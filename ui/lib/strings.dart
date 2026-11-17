/// User-facing strings.
///
/// Deliberately terse: the layout carries the meaning, so this file holds
/// labels and states rather than explanations. Anything that reads like a
/// slogan belongs in the docs, not in the window.
library;

class Strings {
  const Strings(this.chinese);

  final bool chinese;

  static const Strings zh = Strings(true);
  static const Strings en = Strings(false);

  String t(String zh, String en) => chinese ? zh : en;

  // Navigation -----------------------------------------------------------------

  // Top-level sections. Status also carries the machine-level controls that
  // used to live in a separate System page: they describe the same runtime.
  String get navStatus => t('状态', 'Status');
  String get navResolvers => t('解析', 'Resolvers');
  String get navRuleSets => t('规则集', 'Rule sets');
  String get navPreferred => t('优选', 'Preferred');
  String get navJson => 'JSON';

  // Runtime and system sections on the status page.
  String get preferences => t('偏好', 'Preferences');

  // Shared actions -------------------------------------------------------------

  String get save => t('保存', 'Save');
  String get reload => t('重新加载', 'Reload');
  String get validate => t('校验', 'Validate');
  String get add => t('添加', 'Add');
  String get remove => t('删除', 'Remove');
  String get cancel => t('取消', 'Cancel');
  String get proceed => t('继续', 'Continue');
  String get saved => t('已保存', 'Saved');
  String get unsaved => t('未保存', 'Unsaved');
  String get invalid => t('配置无效', 'Invalid');
  String get checking => t('检查中', 'Checking');

  // Status ---------------------------------------------------------------------

  String get engine => t('DNS 引擎', 'DNS engine');
  String get systemDns => t('系统 DNS', 'System DNS');
  String get running => t('运行中', 'Running');
  String get stopped => t('已停止', 'Stopped');
  String get autostart => t('登录启动', 'At login');
  String get managed => t('已接管', 'Managed');
  String get notManaged => t('未接管', 'Not managed');
  String get startEngine => t('启动', 'Start');
  String get stopEngine => t('停止', 'Stop');
  String get restartEngine => t('重启', 'Restart');
  String get refresh => t('刷新', 'Refresh');
  String get noLayers => t('还没有解析层', 'No layers yet');
  String get entryMissing => t('入口层不存在', 'Entry layer is missing');

  // Resolvers ------------------------------------------------------------------

  String get listener => t('监听', 'Listener');
  String get entry => t('入口', 'Entry');
  String get requestTimeout => t('请求超时', 'Timeout');
  String get rangeRefresh => t('号段刷新', 'Range refresh');
  String get allowRemote => t('允许远程', 'Allow remote');
  String get layers => t('解析层', 'Layers');
  String get tag => t('标签', 'Tag');
  String get type => t('类型', 'Type');
  String get next => t('未命中', 'On miss');
  String get fallback => t('失败回退', 'On failure');
  String get plugin => t('插件', 'Plugin');
  String get address => t('地址', 'Address');
  String get url => 'URL';
  String get serverName => t('服务名', 'Server name');
  String get timeout => t('超时', 'Timeout');
  String get layerRefresh => t('刷新', 'Refresh');
  String get matchMode => t('匹配', 'Match');
  String get keywords => t('关键词', 'Keywords');
  String get matchRuleSets => t('规则集', 'Rule sets');
  String get labelMode => t('完整标签', 'Full label');
  String get containsMode => t('包含', 'Contains');
  String get localLayer => t('动态本地', 'Dynamic local');

  // Rule sets ------------------------------------------------------------------

  String get ruleSets => t('规则集', 'Rule sets');
  String get noRuleSets => t('还没有规则集', 'No rule sets yet');
  String get source => t('来源', 'Source');
  String get interval => t('间隔', 'Interval');
  String get remote => t('远程 SRS', 'Remote SRS');
  String get local => t('本地 SRS', 'Local SRS');
  String get downloadTimeout => t('下载超时', 'Download timeout');

  // Preferred ------------------------------------------------------------------

  String get plugins => t('优选插件', 'Preferred plugins');
  String get noPlugins => t('还没有插件', 'No plugins yet');
  String get rewriteTtl => t('重写 TTL', 'Rewrite TTL');
  String get preferredIpv4 => 'IPv4';
  String get preferredIpv6 => 'IPv6';
  String get scheduled => t('定时优选', 'Scheduled');
  String get probe => t('探测', 'Probe');
  String get host => t('主机', 'Host');
  String get path => t('路径', 'Path');
  String get port => t('端口', 'Port');
  String get concurrency => t('并发', 'Concurrency');
  String get samples => t('每 CIDR 采样', 'Samples per CIDR');
  String get probes => t('每候选探测', 'Probes per candidate');
  String get maxCandidates => t('候选上限', 'Max candidates');
  String get candidates => t('候选', 'Candidates');
  String get compatibility => t('兼容主机', 'Compatibility hosts');
  String get excluded => t('排除', 'Excluded');

  // JSON -----------------------------------------------------------------------

  String get configFile => t('配置文件', 'Configuration file');
  String get notCreated => t('尚未创建', 'Not created yet');

  // System ---------------------------------------------------------------------

  String get language => t('语言', 'Language');
  String get appearance => t('风格', 'Appearance');
  String get followSystem => t('跟随系统', 'Follow system');
  String get dark => t('深色', 'Dark');
  String get light => t('浅色', 'Light');
  String get openAtLogin => t('登录启动', 'Open at login');
  String get enableAutoStart => t('启用', 'Enable');
  String get disableAutoStart => t('关闭', 'Disable');
  String get autostartOn => t('已启用', 'Enabled');
  String get autostartOff => t('未启用', 'Disabled');
  String get autostartNeedsBundle =>
      t('需要以 EdgeSteer.app 运行', 'Requires EdgeSteer.app');
  String get enableSystemDns => t('接管', 'Take over');
  String get disableSystemDns => t('恢复自动', 'Restore automatic');
  String get needsPort53 => t('需要回环地址的 53 端口', 'Needs loopback port 53');
  String get needsEngine => t('先启动引擎', 'Start the engine first');
  String get needsListener => t('等待监听器就绪', 'Waiting for the listener');
  String get ready => t('可以接管', 'Ready');
  String get legacyService => t('旧版服务', 'Legacy service');
  String get removeLegacyService => t('移除', 'Remove');
  String get networkServices => t('网络服务', 'Network services');
  String get noNetworkServices => t('无可接管的服务', 'No manageable services');
  String get quit => t('退出 EdgeSteer', 'Quit EdgeSteer');

  // Feedback -------------------------------------------------------------------

  String get operationInProgress => t('操作进行中', 'Operation in progress');
  String get agentUnavailable =>
      t('无法连接菜单栏 Agent', 'Cannot reach the menu-bar agent');
  String get confirm => t('确认', 'Confirm');

  String confirmFor(AgentAction action) => switch (action) {
        AgentAction.startEngine => t(
            '启动 DNS 引擎。监听 53 端口时 macOS 会请求管理员授权。',
            'Starts the DNS engine. macOS asks for authorization on port 53.',
          ),
        AgentAction.stopEngine => t(
            '先恢复系统 DNS，再停止引擎。',
            'Restores system DNS, then stops the engine.',
          ),
        AgentAction.restartEngine => t(
            '重启引擎，进行中的解析会短暂中断。',
            'Restarts the engine. In-flight lookups are briefly interrupted.',
          ),
        AgentAction.enableSystemDns => t(
            '让符合条件的物理网络服务使用本机 DNS。',
            'Points eligible physical network services at the local listener.',
          ),
        AgentAction.disableSystemDns => t(
            '把 EdgeSteer 接管的服务恢复为自动 DNS。',
            'Restores EdgeSteer-managed services to automatic DNS.',
          ),
        AgentAction.enableAutoStart => t(
            '登录时打开 EdgeSteer.app。',
            'Opens EdgeSteer.app at login.',
          ),
        AgentAction.disableAutoStart => t(
            '从登录项中移除 EdgeSteer。',
            'Removes EdgeSteer from login items.',
          ),
        AgentAction.removeLegacyService => t(
            '移除旧版 root DNS 守护进程，macOS 会请求管理员授权。',
            'Removes the legacy root DNS daemon. macOS asks for authorization.',
          ),
        AgentAction.quit => t(
            '恢复系统 DNS 后退出菜单栏服务。',
            'Restores system DNS, then quits the menu-bar service.',
          ),
      };
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
        AgentAction.quit => strings.quit,
      };
}
