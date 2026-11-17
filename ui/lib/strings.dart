/// All user-facing strings, in Simplified Chinese and English.
///
/// The previous native interface shipped both languages, so the Flutter
/// interface keeps them: the language is stored in the agent's preference file
/// and the menu bar follows it.
library;

class Strings {
  const Strings(this.chinese);

  final bool chinese;

  static const Strings zh = Strings(true);
  static const Strings en = Strings(false);

  String get language => chinese ? '简体中文' : 'English';

  String t(String zh, String en) => chinese ? zh : en;

  // Navigation -----------------------------------------------------------------

  String get navOverview => t('状态', 'Status');
  String get navResolvers => t('解析层', 'Resolvers');
  String get navRuleSets => t('规则集', 'Rule sets');
  String get navCloudflare => t('CF 优选', 'CF preferred');
  String get navJson => t('高级 JSON', 'Advanced JSON');
  String get navSystem => t('系统', 'System');
  String get saveConfiguration => t('保存配置', 'Save configuration');

  // Overview -------------------------------------------------------------------

  String get overviewTitle => t('运行状态', 'Runtime status');
  String get overviewCopy => t(
        'EdgeSteer 在菜单栏持续运行；关闭此窗口只释放界面，DNS 服务继续工作。',
        'EdgeSteer keeps running from the menu bar. Closing this window releases '
            'only the interface; DNS service continues.',
      );
  String get dnsEngine => t('DNS 引擎', 'DNS engine');
  String get dnsListener => t('DNS 监听器', 'DNS listener');
  String get systemDns => t('系统 DNS', 'System DNS');
  String get running => t('运行中', 'Running');
  String get stopped => t('已停止', 'Stopped');
  String get managed => t('已接管', 'Managed');
  String get notManaged => t('未接管', 'Not managed');
  String get checking => t('正在检查', 'Checking');
  String get resolverChain => t('解析链', 'Resolver chain');
  String get startEngine => t('启动 DNS 引擎', 'Start DNS engine');
  String get stopEngine => t('停止 DNS 引擎', 'Stop DNS engine');
  String get restartEngine => t('重启', 'Restart');
  String get refresh => t('刷新', 'Refresh');
  String get runtimeHint => t(
        '运行操作通过常驻菜单栏 Agent 执行。',
        'Runtime operations are handled by the resident menu-bar agent.',
      );

  // Resolvers ------------------------------------------------------------------

  String get resolversTitle => t('解析层', 'Resolver layers');
  String get resolversCopy => t(
        '按入口顺序编辑动态本地 DNS、DoH、DoT 和 TCP/UDP 层；每层可配置匹配、下一层与故障回退。',
        'Edit dynamic local DNS, DoH, DoT, TCP, and UDP layers with matching, '
            'next, and failure fallback.',
      );
  String get entryAndListener => t('入口与监听', 'Entry and listener');
  String get listenerAddress => t('监听地址', 'Listener address');
  String get entryLayer => t('入口层', 'Entry layer');
  String get requestTimeout => t('请求超时（毫秒）', 'Request timeout (ms)');
  String get rangeRefresh => t('CF 号段刷新（秒）', 'Cloudflare range refresh (s)');
  String get allowRemote => t('允许远程访问监听器', 'Allow remote listener access');
  String get addLayer => t('添加层', 'Add layer');
  String get moveUp => t('上移', 'Move up');
  String get moveDown => t('下移', 'Move down');
  String get remove => t('删除', 'Remove');
  String get layerTag => t('层标签', 'Layer tag');
  String get layerType => t('层类型', 'Layer type');
  String get nextLayer => t('下一层（未命中时）', 'Next layer on filter miss');
  String get fallbackLayer => t('回退层（请求失败时）', 'Fallback layer on failure');
  String get responsePlugin => t('响应插件', 'Response plugin');
  String get upstreamAddress => t('上游地址', 'Upstream address');
  String get dohUrl => 'DoH URL';
  String get dotServerName => t('DoT 服务名', 'DoT server name');
  String get layerTimeout => t('层超时（毫秒）', 'Layer timeout (ms)');
  String get localRefresh => t('本地 DNS 刷新（秒）', 'Local DNS refresh (s)');
  String get matchMode => t('匹配模式', 'Match mode');
  String get keywords => t('关键词（逗号分隔）', 'Keywords (comma separated)');
  String get ruleSetTags => t('规则集标签（逗号分隔）', 'Rule-set tags (comma separated)');

  // Rule sets ------------------------------------------------------------------

  String get ruleSetsTitle => t('规则集', 'Rule sets');
  String get ruleSetsCopy => t(
        '管理本地或远程 SRS 规则集，并将标签绑定到解析层匹配。',
        'Manage local or remote SRS sources used by resolver matching.',
      );
  String get addRuleSet => t('添加规则集', 'Add rule set');
  String get ruleSetType => t('规则集类型', 'Rule-set type');
  String get noRuleSets => t(
        '还没有规则集。规则集标签用于解析层的 match.rule_sets 绑定。',
        'No rule sets yet. A rule-set tag binds to a resolver layer through '
            'match.rule_sets.',
      );
  String get ruleSetTag => t('规则集标签', 'Rule-set tag');
  String get ruleSetSource => t('规则集来源', 'Rule-set source');
  String get refreshInterval => t('刷新间隔（秒）', 'Refresh interval (s)');
  String get downloadTimeout => t('下载超时（毫秒）', 'Download timeout (ms)');

  // Cloudflare -----------------------------------------------------------------

  String get cloudflareTitle => t('CF 优选', 'CF preferred');
  String get cloudflareCopy => t(
        '配置 Cloudflare 响应重写、固定优选地址和稳定优选探测范围。',
        'Configure Cloudflare response rewriting, preferred addresses, and '
            'stable probing.',
      );
  String get addPlugin => t('添加插件', 'Add plugin');
  String get pluginType => t('插件类型', 'Plugin type');
  String get noPlugins => t(
        '还没有插件。添加 cloudflare_preferred 插件后可在解析层引用它。',
        'No plugins yet. Add a cloudflare_preferred plugin to reference from a '
            'resolver layer.',
      );
  String get noLayers => t(
        '还没有解析层。至少需要一个层，入口层必须存在。',
        'No resolver layers yet. At least one layer is required and the entry '
            'layer must exist.',
      );
  String get pluginTag => t('插件标签', 'Plugin tag');
  String get rewriteTtl => t('重写 TTL（秒）', 'Rewrite TTL (s)');
  String get preferredIpv4 => t('固定优选 IPv4（可选）', 'Preferred IPv4 (optional)');
  String get preferredIpv6 => t('固定优选 IPv6（可选）', 'Preferred IPv6 (optional)');
  String get enableOptimizer => t('启用定时优选', 'Enable scheduled probing');
  String get probeInterval => t('优选间隔（秒）', 'Probe interval (s)');
  String get probeHost => t('探测主机', 'Probe host');
  String get probePath => t('探测路径', 'Probe path');
  String get probePort => t('探测端口', 'Probe port');
  String get probeTimeout => t('探测超时（毫秒）', 'Probe timeout (ms)');
  String get concurrency => t('并发数', 'Concurrency');
  String get samplesPerCidr => t('每个 CIDR 采样数', 'Samples per CIDR');
  String get probesPerCandidate => t('每个候选探测数', 'Probes per candidate');
  String get maxCandidates => t('最大候选数', 'Maximum candidates');
  String get candidateList => t('候选 IP/CIDR（逗号分隔）', 'Candidate IPs/CIDRs (comma separated)');
  String get compatibilityHosts =>
      t('兼容性主机（逗号分隔）', 'Compatibility hosts (comma separated)');
  String get excludedCandidates =>
      t('排除 IP/CIDR（逗号分隔）', 'Excluded IPs/CIDRs (comma separated)');

  // JSON -----------------------------------------------------------------------

  String get jsonTitle => t('高级 JSON', 'Advanced JSON');
  String get jsonCopy => t(
        '直接编辑完整配置。表单页和这里共享同一份严格校验，适合批量修改未暴露字段。',
        'Edit the complete configuration directly. The form pages and this '
            'editor share one strict validation boundary.',
      );
  String get configurationFile => t('配置文件', 'Configuration file');
  String get reload => t('重新加载', 'Reload');
  String get validate => t('校验', 'Validate');

  // System ---------------------------------------------------------------------

  String get systemTitle => t('系统', 'System');
  String get systemCopy => t(
        '菜单栏是主控制面；关闭此窗口只会释放界面进程，DNS 引擎继续运行。',
        'The menu bar is the primary control surface. Closing this window '
            'releases only the interface process; the DNS engine keeps running.',
      );
  String get application => t('应用', 'Application');
  String get applicationCopy =>
      t('选择界面语言与黑白风格。', 'Choose the interface language and appearance.');
  String get appearance => t('界面风格', 'Appearance');
  String get dark => t('深色', 'Dark');
  String get light => t('浅色', 'Light');
  String get openAtLogin => t('登录启动', 'Open at login');
  String get enableAutoStart => t('启用登录启动', 'Enable open at login');
  String get disableAutoStart => t('关闭登录启动', 'Disable open at login');
  String get autostartEnabled =>
      t('已启用：登录时会打开已安装的 EdgeSteer App。',
          'Enabled: the installed EdgeSteer App opens at login.');
  String get autostartDisabled =>
      t('当前未启用登录启动。', 'Open at login is disabled.');
  String get autostartUnsupported => t(
        '安装并打开 EdgeSteer.app 后才能启用登录启动。',
        'Install and open EdgeSteer.app before enabling open at login.',
      );
  String get enableSystemDns => t('启用 EdgeSteer DNS', 'Enable EdgeSteer DNS');
  String get disableSystemDns => t('恢复自动 DNS', 'Restore automatic DNS');
  String get systemDnsNeedsPort53 => t(
        '系统 DNS 需要回环地址的 53 端口监听器。',
        'System DNS requires a loopback listener on port 53.',
      );
  String get systemDnsNeedsEngine =>
      t('请先启动 DNS 引擎。', 'Start the DNS engine first.');
  String get systemDnsNeedsListener => t(
        '正在等待 DNS 监听器就绪。',
        'Waiting for the DNS listener to become ready.',
      );
  String get systemDnsReady =>
      t('监听器已就绪，可以接管系统 DNS。', 'The listener is ready for system DNS.');
  String get systemDnsManaged => t(
        'EdgeSteer 正在接管它记录的物理网络服务；解除时会恢复自动 DNS。',
        'EdgeSteer manages its recorded physical network services and restores '
            'automatic DNS when disabled.',
      );
  String get fixConfigFirst => t(
        '请先修复配置校验错误。',
        'Fix configuration validation errors first.',
      );
  String get legacyService => t('旧版服务', 'Legacy service');
  String get legacyServiceCopy => t(
        '检测到旧版命令行服务；仅在确认后移除。',
        'A legacy command-line service was detected; it is removed only after '
            'confirmation.',
      );
  String get removeLegacyService => t('移除旧版服务', 'Remove legacy service');
  String get networkServices => t('网络服务', 'Network services');
  String get noNetworkServices => t(
        '当前平台未提供可接管的物理网络服务。',
        'This platform does not report physical network services EdgeSteer can '
            'manage.',
      );
  String get quitApplication => t('退出 EdgeSteer', 'Quit EdgeSteer');

  // Confirmations --------------------------------------------------------------

  String get confirmTitle => t('确认操作', 'Confirm action');
  String get cancel => t('取消', 'Cancel');
  String get continueLabel => t('继续', 'Continue');
  String get operationInProgress => t('已有操作正在执行。', 'An operation is already in progress.');
  String get configurationSaved => t('配置已保存。', 'Configuration saved.');
  String get configurationReloaded => t('配置已重新加载。', 'Configuration reloaded.');
  String get configurationValid => t('配置有效。', 'Configuration is valid.');
  String get agentUnavailable =>
      t('EdgeSteer Agent 不可用。', 'EdgeSteer Agent is unavailable.');

  String confirmFor(String action) => t(
        '将执行：$action。',
        'This will run: $action.',
      );
}

/// Actions the interface can ask the agent to perform.
enum AgentAction {
  startEngine,
  stopEngine,
  restartEngine,
  enableSystemDns,
  disableSystemDns,
  enableAutoStart,
  disableAutoStart,
  removeLegacyService,
  quit;

  bool get requiresValidConfiguration =>
      this == startEngine ||
      this == restartEngine ||
      this == enableSystemDns ||
      this == enableAutoStart;

  bool get isDestructive =>
      this == stopEngine ||
      this == disableSystemDns ||
      this == disableAutoStart ||
      this == removeLegacyService ||
      this == quit;

  String label(Strings strings) => switch (this) {
        AgentAction.startEngine => strings.startEngine,
        AgentAction.stopEngine => strings.stopEngine,
        AgentAction.restartEngine => strings.restartEngine,
        AgentAction.enableSystemDns => strings.enableSystemDns,
        AgentAction.disableSystemDns => strings.disableSystemDns,
        AgentAction.enableAutoStart => strings.enableAutoStart,
        AgentAction.disableAutoStart => strings.disableAutoStart,
        AgentAction.removeLegacyService => strings.removeLegacyService,
        AgentAction.quit => strings.quitApplication,
      };

  String confirmation(Strings strings) => switch (this) {
        AgentAction.startEngine => strings.t(
            '将启动由 EdgeSteer App 管理的 DNS 引擎。监听 53 端口时，macOS 可能请求管理员授权。',
            'This starts the DNS engine managed by EdgeSteer. macOS may request '
                'authorization for port 53.',
          ),
        AgentAction.stopEngine => strings.t(
            '将先恢复 EdgeSteer 接管的系统 DNS，再停止 DNS 引擎。',
            'This restores EdgeSteer-managed system DNS before stopping the DNS '
                'engine.',
          ),
        AgentAction.restartEngine => strings.t(
            '将重启 DNS 引擎。已有解析请求可能短暂中断。',
            'This restarts the DNS engine. Existing DNS requests may be briefly '
                'interrupted.',
          ),
        AgentAction.enableSystemDns => strings.t(
            '将让符合条件的物理网络服务使用 EdgeSteer 的回环 DNS。',
            'This changes eligible physical network services to use EdgeSteer\'s '
                'loopback DNS.',
          ),
        AgentAction.disableSystemDns => strings.t(
            '将只恢复 EdgeSteer 自己接管的 DNS 服务到自动 DNS。',
            'This restores only the DNS services currently managed by EdgeSteer to '
                'automatic DNS.',
          ),
        AgentAction.enableAutoStart => strings.t(
            '登录时会启动已安装的 EdgeSteer App，不会安装独立命令行服务。',
            'This opens the installed EdgeSteer App at login without installing a '
                'separate command-line service.',
          ),
        AgentAction.disableAutoStart => strings.t(
            '将从当前用户的登录启动项中移除 EdgeSteer。',
            'This removes EdgeSteer from the current user\'s login items.',
          ),
        AgentAction.removeLegacyService => strings.t(
            '将移除旧版 root DNS 守护进程。macOS 可能请求管理员授权。',
            'This removes the legacy root DNS daemon. macOS may request '
                'authorization.',
          ),
        AgentAction.quit => strings.t(
            'EdgeSteer 会先恢复其接管的系统 DNS，再停止 DNS 引擎和菜单栏服务。',
            'EdgeSteer restores the system DNS it manages, then stops the DNS '
                'engine and menu-bar service.',
          ),
      };
}
