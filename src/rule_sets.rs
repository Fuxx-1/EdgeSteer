use std::{
    collections::HashMap,
    io::{Cursor, Read},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use flate2::read::ZlibDecoder;
use regex::Regex;
use reqwest::redirect::Policy;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::{
    config::{RuleSetConfig, RuleSetType},
    state::SharedState,
};

const MAX_COMPRESSED_RULE_SET_BYTES: usize = 16 * 1024 * 1024;
const MAX_UNCOMPRESSED_RULE_SET_BYTES: usize = 64 * 1024 * 1024;
const MAX_RULES: usize = 500_000;
const MAX_LOGICAL_RULE_DEPTH: usize = 100;
const MAX_RULE_ITEM_VALUES: usize = 500_000;

// sing-box common/srs/binary.go rule item values. EdgeSteer deliberately
// accepts only domain conditions because it routes DNS queries and has no
// process, interface, port, or destination IP metadata to evaluate.
const RULE_ITEM_DOMAIN: u8 = 2;
const RULE_ITEM_DOMAIN_KEYWORD: u8 = 3;
const RULE_ITEM_DOMAIN_REGEX: u8 = 4;
const RULE_ITEM_FINAL: u8 = u8::MAX;
const DOMAIN_PREFIX_LABEL: u8 = b'\r';
const DOMAIN_ROOT_LABEL: u8 = b'\n';

/// The atomically published collection used during DNS layer selection.
#[derive(Debug, Default)]
pub struct RuleSetStore {
    sets: HashMap<String, LoadedRuleSet>,
}

impl RuleSetStore {
    fn new(sets: HashMap<String, LoadedRuleSet>) -> Self {
        Self { sets }
    }

    /// A stored set must originate from the currently selected configuration
    /// source. This prevents a just-reloaded config from briefly using an old
    /// set that happened to retain the same tag.
    pub fn matches(
        &self,
        tag: &str,
        expected_source: Option<&RuleSetConfig>,
        domain: &str,
    ) -> bool {
        self.sets
            .get(tag)
            .zip(expected_source)
            .is_some_and(|(loaded, expected)| {
                loaded.source == *expected && loaded.rules.matches(domain)
            })
    }
}

#[derive(Debug)]
struct LoadedRuleSet {
    source: RuleSetConfig,
    rules: Arc<DomainRuleSet>,
}

/// Continuously refreshes configured local and remote sing-box SRS rule sets.
/// A failed refresh never evicts an already loaded rule set from the active
/// store; a new or changed source remains unmatched until it loads correctly.
pub async fn refresh_loop(state: SharedState) {
    let mut worker = RefreshWorker::default();
    loop {
        let delay = worker.refresh(&state).await;
        tokio::select! {
            _ = sleep(delay) => {}
            _ = state.config_changed.notified() => {}
        }
    }
}

#[derive(Default)]
struct RefreshWorker {
    entries: HashMap<String, RefreshEntry>,
}

struct RefreshEntry {
    source: RuleSetConfig,
    rules: Option<Arc<DomainRuleSet>>,
    refresh_at: Instant,
}

impl RefreshWorker {
    async fn refresh(&mut self, state: &SharedState) -> Duration {
        let config = state.runtime.load_full().config.clone();
        self.entries.retain(|tag, _| config.rule_set(tag).is_some());

        let now = Instant::now();
        for source in &config.rule_sets {
            let unchanged = self
                .entries
                .get(&source.tag)
                .is_some_and(|entry| entry.source == *source);
            let due = self
                .entries
                .get(&source.tag)
                .is_none_or(|entry| now >= entry.refresh_at);
            if unchanged && !due {
                continue;
            }

            let retained_rules = unchanged
                .then(|| {
                    self.entries
                        .get(&source.tag)
                        .and_then(|entry| entry.rules.clone())
                })
                .flatten();
            let rules = match load_rule_set(source).await {
                Ok(rules) => {
                    let rules = Arc::new(rules);
                    info!(
                        rule_set = %source.tag,
                        rules = rules.rule_count(),
                        "loaded sing-box domain rule set"
                    );
                    Some(rules)
                }
                Err(error) => {
                    warn!(
                        rule_set = %source.tag,
                        %error,
                        "could not refresh sing-box domain rule set; keeping the active version"
                    );
                    retained_rules
                }
            };
            self.entries.insert(
                source.tag.clone(),
                RefreshEntry {
                    source: source.clone(),
                    rules,
                    refresh_at: Instant::now() + Duration::from_secs(source.update_interval_secs()),
                },
            );
        }

        let mut active = HashMap::new();
        for (tag, entry) in &self.entries {
            if let Some(rules) = &entry.rules {
                active.insert(
                    tag.clone(),
                    LoadedRuleSet {
                        source: entry.source.clone(),
                        rules: rules.clone(),
                    },
                );
            }
        }
        state.replace_rule_sets(RuleSetStore::new(active));

        self.entries
            .values()
            .map(|entry| entry.refresh_at.saturating_duration_since(Instant::now()))
            .min()
            .unwrap_or(Duration::from_secs(60))
            .max(Duration::from_secs(1))
    }
}

async fn load_rule_set(source: &RuleSetConfig) -> Result<DomainRuleSet> {
    let bytes = match source.kind {
        RuleSetType::Local => {
            let path = source.local_path();
            let metadata = tokio::fs::metadata(path)
                .await
                .with_context(|| format!("inspect local rule set {}", path.display()))?;
            ensure!(
                metadata.len() <= MAX_COMPRESSED_RULE_SET_BYTES as u64,
                "local rule set {} exceeds the {} byte limit",
                path.display(),
                MAX_COMPRESSED_RULE_SET_BYTES
            );
            tokio::fs::read(path)
                .await
                .with_context(|| format!("read local rule set {}", path.display()))?
        }
        RuleSetType::Remote => {
            let endpoint = source.endpoint()?;
            let client = reqwest::Client::builder()
                // A download must use the system/TUN network path, not an
                // inherited HTTP proxy setting that could create hidden
                // configuration-dependent routing behavior.
                .no_proxy()
                .redirect(Policy::none())
                .timeout(Duration::from_millis(source.timeout_ms()))
                .user_agent("edgesteer/0.3")
                .build()
                .context("create rule-set download client")?;
            let response = client
                .get(endpoint.clone())
                .send()
                .await
                .with_context(|| format!("download rule set {endpoint}"))?
                .error_for_status()
                .with_context(|| format!("read HTTP status for rule set {endpoint}"))?;
            if response
                .content_length()
                .is_some_and(|length| length > MAX_COMPRESSED_RULE_SET_BYTES as u64)
            {
                bail!(
                    "remote rule set {endpoint} exceeds the {} byte limit",
                    MAX_COMPRESSED_RULE_SET_BYTES
                );
            }
            let bytes = response
                .bytes()
                .await
                .with_context(|| format!("read rule set body from {endpoint}"))?;
            ensure!(
                bytes.len() <= MAX_COMPRESSED_RULE_SET_BYTES,
                "remote rule set {endpoint} exceeds the {} byte limit",
                MAX_COMPRESSED_RULE_SET_BYTES
            );
            bytes.to_vec()
        }
    };
    parse_sing_box_srs(&bytes)
}

#[derive(Debug)]
struct DomainRuleSet {
    rules: Vec<RuleExpression>,
}

impl DomainRuleSet {
    fn matches(&self, domain: &str) -> bool {
        let domain = domain.trim_end_matches('.').to_ascii_lowercase();
        !domain.is_empty() && self.rules.iter().any(|rule| rule.matches(&domain))
    }

    fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

#[derive(Debug)]
enum RuleExpression {
    Any(Vec<RuleExpression>),
    All(Vec<RuleExpression>),
    Not(Box<RuleExpression>),
    DomainMatcher(SrsDomainMatcher),
    DomainKeyword(Vec<String>),
    DomainRegex(Vec<Regex>),
}

impl RuleExpression {
    fn matches(&self, domain: &str) -> bool {
        match self {
            Self::Any(rules) => rules.iter().any(|rule| rule.matches(domain)),
            Self::All(rules) => rules.iter().all(|rule| rule.matches(domain)),
            Self::Not(rule) => !rule.matches(domain),
            Self::DomainMatcher(matcher) => matcher.matches(domain),
            Self::DomainKeyword(keywords) => {
                keywords.iter().any(|keyword| domain.contains(keyword))
            }
            Self::DomainRegex(expressions) => expressions
                .iter()
                .any(|expression| expression.is_match(domain)),
        }
    }
}

/// Parse the sing-box binary SRS wire format (versions 1 through 5) and keep
/// its domain-only rule semantics. The format is documented by the upstream
/// `common/srs` package; decoding it here avoids a runtime sing-box binary.
fn parse_sing_box_srs(bytes: &[u8]) -> Result<DomainRuleSet> {
    ensure!(
        bytes.len() >= 4,
        "sing-box SRS file is shorter than its header"
    );
    ensure!(&bytes[..3] == b"SRS", "invalid sing-box SRS magic");
    let version = bytes[3];
    ensure!(
        (1..=5).contains(&version),
        "unsupported sing-box SRS version {version}"
    );

    let mut decoder = ZlibDecoder::new(&bytes[4..]);
    let mut payload = Vec::new();
    decoder
        .by_ref()
        .take((MAX_UNCOMPRESSED_RULE_SET_BYTES + 1) as u64)
        .read_to_end(&mut payload)
        .context("decompress sing-box SRS payload")?;
    ensure!(
        payload.len() <= MAX_UNCOMPRESSED_RULE_SET_BYTES,
        "sing-box SRS payload exceeds the {} byte limit",
        MAX_UNCOMPRESSED_RULE_SET_BYTES
    );

    let mut reader = SrsReader::new(&payload);
    let count = reader.read_len(MAX_RULES, "rule count")?;
    let mut rules = Vec::with_capacity(count);
    for index in 0..count {
        rules.push(parse_rule(&mut reader, 0).with_context(|| format!("read SRS rule[{index}]"))?);
    }
    Ok(DomainRuleSet { rules })
}

fn parse_rule(reader: &mut SrsReader<'_>, depth: usize) -> Result<RuleExpression> {
    ensure!(
        depth <= MAX_LOGICAL_RULE_DEPTH,
        "SRS logical rule nesting exceeds {MAX_LOGICAL_RULE_DEPTH}"
    );
    match reader.read_u8()? {
        0 => parse_default_rule(reader),
        1 => parse_logical_rule(reader, depth),
        kind => bail!("unknown SRS rule type {kind}"),
    }
}

fn parse_default_rule(reader: &mut SrsReader<'_>) -> Result<RuleExpression> {
    let mut conditions = Vec::new();
    loop {
        let item_type = reader.read_u8()?;
        match item_type {
            RULE_ITEM_DOMAIN => {
                conditions.push(RuleExpression::DomainMatcher(SrsDomainMatcher::parse(
                    reader,
                )?));
            }
            RULE_ITEM_DOMAIN_KEYWORD => {
                let keywords = reader
                    .read_strings("domain_keyword")?
                    .into_iter()
                    .map(|keyword| keyword.to_ascii_lowercase())
                    .collect();
                conditions.push(RuleExpression::DomainKeyword(keywords));
            }
            RULE_ITEM_DOMAIN_REGEX => {
                let expressions = reader
                    .read_strings("domain_regex")?
                    .into_iter()
                    .map(|expression| {
                        Regex::new(&expression)
                            .with_context(|| format!("compile SRS domain_regex {expression:?}"))
                    })
                    .collect::<Result<Vec<_>>>()?;
                conditions.push(RuleExpression::DomainRegex(expressions));
            }
            RULE_ITEM_FINAL => {
                let invert = reader.read_bool()?;
                ensure!(
                    !conditions.is_empty(),
                    "SRS rule does not contain a domain condition"
                );
                let expression = if conditions.len() == 1 {
                    conditions.pop().expect("checked non-empty conditions")
                } else {
                    // sing-box groups domain, domain_keyword, and domain_regex
                    // as destination-address alternatives inside one default rule.
                    RuleExpression::Any(conditions)
                };
                return Ok(if invert {
                    RuleExpression::Not(Box::new(expression))
                } else {
                    expression
                });
            }
            unsupported => {
                bail!(
                    "SRS rule item {unsupported} is not domain-based; EdgeSteer only accepts domain rule sets"
                );
            }
        }
    }
}

fn parse_logical_rule(reader: &mut SrsReader<'_>, depth: usize) -> Result<RuleExpression> {
    let mode = reader.read_u8()?;
    let count = reader.read_len(MAX_RULES, "logical rule count")?;
    ensure!(count > 0, "SRS logical rule has no child rules");
    let mut rules = Vec::with_capacity(count);
    for index in 0..count {
        rules.push(
            parse_rule(reader, depth + 1)
                .with_context(|| format!("read logical SRS rule[{index}]"))?,
        );
    }
    let expression = match mode {
        0 => RuleExpression::All(rules),
        1 => RuleExpression::Any(rules),
        _ => bail!("unknown SRS logical mode {mode}"),
    };
    Ok(if reader.read_bool()? {
        RuleExpression::Not(Box::new(expression))
    } else {
        expression
    })
}

/// The compact domain trie used by sing-box `domain.Matcher`.
#[derive(Debug)]
struct SrsDomainMatcher {
    leaves: Vec<u64>,
    label_bitmap: Vec<u64>,
    labels: Vec<u8>,
    one_positions: Vec<usize>,
    one_prefix: Vec<usize>,
}

impl SrsDomainMatcher {
    fn parse(reader: &mut SrsReader<'_>) -> Result<Self> {
        // Upstream currently writes this compact-set version as zero and does
        // not branch on it while reading; consume it for wire compatibility.
        let _format_version = reader.read_u8()?;
        let mut leaves = reader.read_u64s("domain matcher leaves")?;
        let label_bitmap = reader.read_u64s("domain matcher label bitmap")?;
        let labels = reader.read_bytes("domain matcher labels")?;

        ensure!(
            !label_bitmap.is_empty(),
            "SRS domain matcher has an empty label bitmap"
        );
        let mut one_positions = Vec::new();
        let mut last_one = None;
        let mut one_prefix = Vec::with_capacity(label_bitmap.len() + 1);
        let mut ones = 0_usize;
        for (word_index, word) in label_bitmap.iter().copied().enumerate() {
            one_prefix.push(ones);
            ones += word.count_ones() as usize;
            if word != 0 {
                last_one = Some(word_index * 64 + (63 - word.leading_zeros() as usize));
            }
            for bit in 0..64 {
                if word & (1_u64 << bit) != 0 {
                    one_positions.push(word_index * 64 + bit);
                }
            }
        }
        one_prefix.push(ones);
        let last_one = last_one.context("SRS domain matcher has no terminator")?;
        let zeroes = last_one + 1 - ones;
        ensure!(
            ones == zeroes + 1,
            "SRS domain matcher has an invalid label bitmap"
        );
        ensure!(
            labels.len() == zeroes,
            "SRS domain matcher label count does not match its bitmap"
        );

        let required_leaf_words = ones.div_ceil(64);
        if leaves.len() < required_leaf_words {
            leaves.resize(required_leaf_words, 0);
        }
        Ok(Self {
            leaves,
            label_bitmap,
            labels,
            one_positions,
            one_prefix,
        })
    }

    fn matches(&self, domain: &str) -> bool {
        let key = domain.chars().rev().collect::<String>();
        let mut node_id = 0_usize;
        let mut bitmap_index = 0_usize;
        for current in key.bytes() {
            loop {
                if self.label_bit(bitmap_index) {
                    return false;
                }
                let Some(&next_label) = self.labels.get(bitmap_index.saturating_sub(node_id))
                else {
                    return false;
                };
                if next_label == DOMAIN_PREFIX_LABEL {
                    return true;
                }
                if next_label == DOMAIN_ROOT_LABEL {
                    let next_node_id = self.count_zeroes(bitmap_index + 1);
                    if current == b'.' && self.leaf_bit(next_node_id) {
                        return true;
                    }
                }
                if next_label == current {
                    break;
                }
                bitmap_index += 1;
            }
            node_id = self.count_zeroes(bitmap_index + 1);
            let Some(next_index) = node_id
                .checked_sub(1)
                .and_then(|index| self.one_positions.get(index).copied())
            else {
                return false;
            };
            bitmap_index = next_index + 1;
        }
        if self.leaf_bit(node_id) {
            return true;
        }
        loop {
            if self.label_bit(bitmap_index) {
                return false;
            }
            let Some(&next_label) = self.labels.get(bitmap_index.saturating_sub(node_id)) else {
                return false;
            };
            if next_label == DOMAIN_PREFIX_LABEL || next_label == DOMAIN_ROOT_LABEL {
                return true;
            }
            bitmap_index += 1;
        }
    }

    fn label_bit(&self, index: usize) -> bool {
        self.label_bitmap
            .get(index / 64)
            .is_some_and(|word| word & (1_u64 << (index % 64)) != 0)
    }

    fn leaf_bit(&self, index: usize) -> bool {
        self.leaves
            .get(index / 64)
            .is_some_and(|word| word & (1_u64 << (index % 64)) != 0)
    }

    fn count_zeroes(&self, end: usize) -> usize {
        let word_index = end / 64;
        let bit_count = end % 64;
        let ones_before = self.one_prefix.get(word_index).copied().unwrap_or_else(|| {
            self.one_prefix
                .last()
                .copied()
                .expect("domain matcher always has a prefix count")
        });
        let ones_in_word = self.label_bitmap.get(word_index).map_or(0, |word| {
            if bit_count == 0 {
                0
            } else {
                (word & ((1_u64 << bit_count) - 1)).count_ones() as usize
            }
        });
        end.saturating_sub(ones_before + ones_in_word)
    }
}

struct SrsReader<'a> {
    cursor: Cursor<&'a [u8]>,
}

impl<'a> SrsReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(bytes),
        }
    }

    fn read_u8(&mut self) -> Result<u8> {
        let mut value = [0_u8; 1];
        self.cursor
            .read_exact(&mut value)
            .context("unexpected end of SRS payload")?;
        Ok(value[0])
    }

    fn read_bool(&mut self) -> Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    fn read_uvarint(&mut self) -> Result<u64> {
        let mut value = 0_u64;
        for index in 0..10 {
            let byte = self.read_u8()?;
            let bits = byte & 0x7f;
            if index == 9 && bits > 1 {
                bail!("SRS unsigned varint overflows u64");
            }
            value |= u64::from(bits) << (index * 7);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        bail!("SRS unsigned varint is too long")
    }

    fn read_len(&mut self, limit: usize, field: &str) -> Result<usize> {
        let value = self.read_uvarint()?;
        let value =
            usize::try_from(value).with_context(|| format!("{field} does not fit usize"))?;
        ensure!(value <= limit, "SRS {field} exceeds the {limit} item limit");
        Ok(value)
    }

    fn read_exact(&mut self, length: usize, field: &str) -> Result<Vec<u8>> {
        let mut value = vec![0_u8; length];
        self.cursor
            .read_exact(&mut value)
            .with_context(|| format!("unexpected end of SRS {field}"))?;
        Ok(value)
    }

    fn read_bytes(&mut self, field: &str) -> Result<Vec<u8>> {
        let length = self.read_len(MAX_UNCOMPRESSED_RULE_SET_BYTES, field)?;
        self.read_exact(length, field)
    }

    fn read_u64s(&mut self, field: &str) -> Result<Vec<u64>> {
        let count = self.read_len(MAX_UNCOMPRESSED_RULE_SET_BYTES / 8, field)?;
        let bytes = self.read_exact(
            count
                .checked_mul(8)
                .context("SRS u64 slice byte length overflow")?,
            field,
        )?;
        Ok(bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_be_bytes(chunk.try_into().expect("exact 8-byte chunk")))
            .collect())
    }

    fn read_strings(&mut self, field: &str) -> Result<Vec<String>> {
        let count = self.read_len(MAX_RULE_ITEM_VALUES, field)?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            let value = self.read_bytes(field)?;
            values.push(
                String::from_utf8(value)
                    .with_context(|| format!("SRS {field} contains invalid UTF-8"))?,
            );
        }
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::{Compression, write::ZlibEncoder};

    use super::*;

    #[test]
    fn parses_and_matches_domain_and_suffix_rules() {
        let binary = make_srs(&[
            make_domain_rule("example.com", false),
            make_domain_rule("internal.test", true),
        ]);
        let rule_set = parse_sing_box_srs(&binary).expect("SRS parses");

        assert!(rule_set.matches("example.com"));
        assert!(!rule_set.matches("www.example.com"));
        assert!(rule_set.matches("internal.test"));
        assert!(rule_set.matches("api.internal.test"));
        assert!(!rule_set.matches("internal.testing"));
    }

    #[test]
    fn rejects_non_domain_srs_rules_without_silent_misrouting() {
        let binary = make_srs(&[vec![0, 5, 0, RULE_ITEM_FINAL, 0]]);
        let error = parse_sing_box_srs(&binary).expect_err("port rule must be rejected");
        assert!(format!("{error:#}").contains("not domain-based"));
    }

    #[test]
    fn store_does_not_use_a_same_tag_rule_set_from_a_replaced_source() {
        let source = remote_source("https://example.com/old.srs");
        let replacement = remote_source("https://example.com/new.srs");
        let rules = Arc::new(DomainRuleSet {
            rules: vec![RuleExpression::DomainKeyword(vec!["private".to_owned()])],
        });
        let store = RuleSetStore::new(HashMap::from([(
            "private".to_owned(),
            LoadedRuleSet {
                source: source.clone(),
                rules,
            },
        )]));

        assert!(store.matches("private", Some(&source), "host.private"));
        assert!(!store.matches("private", Some(&replacement), "host.private"));
    }

    fn remote_source(url: &str) -> RuleSetConfig {
        RuleSetConfig {
            tag: "private".to_owned(),
            kind: RuleSetType::Remote,
            path: None,
            url: Some(url.to_owned()),
            update_interval_secs: Some(60),
            timeout_ms: Some(1_000),
        }
    }

    fn make_srs(rules: &[Vec<u8>]) -> Vec<u8> {
        let mut payload = Vec::new();
        write_uvarint(&mut payload, rules.len() as u64);
        for rule in rules {
            payload.extend_from_slice(rule);
        }

        let mut compressed = ZlibEncoder::new(Vec::new(), Compression::best());
        compressed.write_all(&payload).unwrap();
        let compressed = compressed.finish().unwrap();
        let mut result = b"SRS\x01".to_vec();
        result.extend_from_slice(&compressed);
        result
    }

    fn make_domain_rule(domain: &str, suffix: bool) -> Vec<u8> {
        let mut result = vec![0, RULE_ITEM_DOMAIN];
        let mut key = domain.chars().rev().collect::<String>().into_bytes();
        if suffix {
            key.push(DOMAIN_ROOT_LABEL);
        }
        write_matcher(&mut result, &key);
        result.extend_from_slice(&[RULE_ITEM_FINAL, 0]);
        result
    }

    fn write_matcher(output: &mut Vec<u8>, key: &[u8]) {
        // A single key is a one-child-per-node trie. Each node contributes a
        // zero edge bit then a one terminator bit; the final node is a leaf.
        output.push(0);
        write_uvarint(output, 1);
        output.extend_from_slice(&(1_u64 << key.len()).to_be_bytes());
        write_uvarint(output, 1);
        let bitmap = (0..key.len()).fold(1_u64 << (key.len() * 2), |bitmap, node| {
            bitmap | (1_u64 << (node * 2 + 1))
        });
        output.extend_from_slice(&bitmap.to_be_bytes());
        write_uvarint(output, key.len() as u64);
        output.extend_from_slice(key);
    }

    fn write_uvarint(output: &mut Vec<u8>, mut value: u64) {
        while value >= 0x80 {
            output.push(value as u8 | 0x80);
            value >>= 7;
        }
        output.push(value as u8);
    }
}
