/**
 * Interface text.
 *
 * English is the primitive: every string the UI shows is written in English at its call site, and
 * `ZH` is a lookup from that text to Chinese. Nothing in the components knows a language; a
 * missing translation falls back to the English primitive, which is a visible English word rather
 * than a blank or a key.
 *
 * This is gettext's shape on purpose — the primitive doubles as the source text, so a translator
 * can read the dictionary on its own and a component cannot drift from the string it shows.
 */

export type Lang = 'en' | 'zh';

const STORAGE_KEY = 'sysml-studio.lang';

function stored(): Lang | null {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return value === 'zh' || value === 'en' ? value : null;
  } catch {
    // A browser with storage disabled still gets a working default.
    return null;
  }
}

let current: Lang = stored() ?? 'en';

export function lang(): Lang {
  return current;
}

export function setLang(next: Lang): void {
  current = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    /* the choice lasts for this session only */
  }
}

/** Singular or plural English primitive, so the dictionary can map both to one Chinese word. */
export function plural(n: number, one: string, many: string): string {
  return n === 1 ? one : many;
}

const ZH: Record<string, string> = {
  // ---- the ladder ----------------------------------------------------
  'What the model is waiting for': '模型在等什么',
  'requirements': '需求',
  'questions': '分岔点',
  'allocation': '分配',
  'values': '参数',
  'interfaces': '接口',
  'verification': '验证',
  'ready': '完成',
  'nothing outstanding': '没有待办',
  'Blocking stage {n}': '挡住第 {n} 阶段',
  'the constraint has only constants in it': '约束里只有常量，没有可判的东西',
  'nothing in the model carries this requirement': '没有任何结构承担这条需求',
  'this value has no ledger entry': '这个值没有台账记录',
  'one end of this connection does not exist': '连接的一端不存在',
  'this requirement has never been judged': '需求还没有判过',
  'no requirement is derived from this concern': '这条 concern 没有需求从它派生',
  'Go to this element': '跳到这个元素',
  'Copy the command': '复制这条命令',
  'Copied': '已复制',
  'Copy failed': '复制失败',
  // ---- toolbar -------------------------------------------------------
  'general view: the structure diagram': '通用视图：结构图',
  'interconnection view: parts and connectors': '互连图：部件与连接',
  'traceability view: requirements and implementers, unmet marked red': '追溯视图：需求与实现者，悬空需求标红',
  'package view: the namespace tree, packages as containers': '包视图：命名空间树，包画成容器',
  'state transition view: states and the transitions between them': '状态转移视图：状态与它们之间的转移',
  'action flow view: actions and the successions between them': '动作流视图：动作与它们之间的顺序',
  'sequence view: parts as lifelines, messages in the order they are written': '时序视图：部件作生命线，消息按书写顺序排列',
  'extended: satisfy edges and inherited ports': 'extended：含 satisfy 边与继承的端口',
  'compat: mirrors the reference implementation node by node': 'compat：逐节点复刻参考实现',
  'Zoom to fit': '缩放到适合窗口',
  'Clear every manual position': '清除所有手动位置',
  'Not loaded': '未加载',
  'Interface language': '界面语言',

  // ---- canvas --------------------------------------------------------
  'The interconnection view draws instances only. This model declares definitions and no part usages.':
    '互连视图只画实例。这个模型只声明了定义，没有 part 用法。',
  'Nothing in this model can be drawn.': '模型里没有可画的元素。',
  'Drag a box to move it, drag its corner to resize, scroll to zoom, drag the background to pan':
    '拖动节点移动，拖角改大小，滚轮缩放，空白处拖动平移',
  'The model changed on disk. The picture has been reloaded.': '模型已在外部修改，画面已更新',
  'No view draws this element.': '这个元素没有画在任何视图里',

  // ---- counts --------------------------------------------------------
  element: '元素',
  elements: '元素',
  relationship: '关系',
  relationships: '关系',
  error: 'error',
  warning: 'warning',
  '{n} element': '{n} 个元素',
  '{n} elements': '{n} 个元素',
  '{n} relationship': '{n} 条关系',
  '{n} relationships': '{n} 条关系',
  '{n} error': '{n} error',
  '{n} errors': '{n} error',
  '{n} warning': '{n} warning',
  '{n} warnings': '{n} warning',
  'value with no record': '未记账的值',
  'values with no record': '未记账的值',
  'unapproved assumption': '未批准的 assumed',
  'unapproved assumptions': '未批准的 assumed',
  'open question': 'open 问题',
  'open questions': 'open 问题',
  '{n} more file': '另有 {n} 个文件',
  '{n} more files': '另有 {n} 个文件',

  // ---- inspector -----------------------------------------------------
  'Nothing selected': '未选择元素',
  'Nothing is waiting: every value has a record, every assumption is approved, no question is open.': '没有待办：每个值都有来源，每个假设都已批准，没有未决问题。',
  'Click a box to see its properties, provenance and problems.':
    '点击画布上的方框查看属性、来源与问题。',
  'Value and provenance': '数值与来源',
  'Current value': '当前值',
  basis: '依据',
  falsifier: '反证',
  'cited sources': '来源',
  'Approve (only a person can approve an assumption)': '批准（只有人能批准假定）',
  'your name': '你的名字',
  unapproved: '未批准',
  Approve: '批准',
  'No ledger entry records this number. Say where it came from and the gate will pass it.':
    '这个数字在台账里没有记录。写下它从哪来，门禁才会放行。',
  'Record a source': '记一条来源',
  'spec — the document states this value': 'spec — 文档写明了这个值',
  'derived — computed from other values': 'derived — 由其他值算出',
  'assumed — my judgement': 'assumed — 我的判断',
  'basis, one sentence': '依据，一句话',
  'falsifier: what observation would show it is wrong': '反证：观察到什么说明它错了',
  'source id (spec requires one, e.g. S001)': '来源编号（spec 必填，如 S001）',
  Record: '记录',
  Identity: '标识',
  'Reset to automatic position': '恢复自动位置',
  Rename: '改名',
  Members: '成员',
  'Problems {n}': '问题 {n}',
  'Relationships {n}': '关系 {n}',
  '(unresolved)': '（未解析）',
  'Add a member': '添加成员',
  name: '名字',
  Add: '添加',
  'Writes into the model text and re-parses immediately. Nothing is written if it does not parse.':
    '写入模型的文本，会立即重新解析校验；不通过则不落盘。',
  'Fill in a name': '填写名字',
  'The write was refused': '写入失败',
  'Approval failed': '批准失败',
  'Recording failed': '记录失败',
  'Adding failed': '添加失败',
  'Rename failed': '改名失败',
  'Reset failed': '重置失败',
  'The ledger changed. Reload to see the new state.': '台账已变化，请重新加载。',

  // ---- source --------------------------------------------------------
  'Select an element: click a box on the canvas or an entry in the navigator.':
    '未选择元素。点击画布上的方框或左侧导航里的条目。',
  'Reading…': '读取中…',
  'Could not read the file': '读取失败',

  // ---- model-declared views ------------------------------------------
  'Views the model declares': '模型声明的视图',
  'Draw the whole model': '画整个模型',
  'whole model': '整个模型',
  'drawn with the current projection': '用当前投影绘制',
  'No projection draws {kind}': '没有投影能画 {kind}',
  'an unnamed type': '未命名的类型',

  // ---- review queue --------------------------------------------------
  Review: '评审',
  'Nothing to review.': '没有待处理项。',
  stale: '已过期',
  'Re-judge': '重新判定',
  'Re-judging failed': '重新判定失败',
  'j / k moves, Enter acts on the selected row.': 'j / k 移动，Enter 处理当前行。',

  // ---- change marks --------------------------------------------------
  'Change marks': '变更标记',
  'Show what changed since this view was opened': '显示这个视图打开后发生的变化',
  'Take the current picture as the new baseline': '以当前画面作为新的基线',

  // ---- verdicts ------------------------------------------------------
  Verdict: '判定',
  Quantity: '量纲',
  'Comes to': '结果为',
  'Take back the last edit (Ctrl+Z)': '撤销上一步编辑（Ctrl+Z）',
  'The last edit was taken back': '已撤销上一步编辑',
  'Nothing to take back': '没有可撤销的编辑',
  'Put back the edit an undo took away (Ctrl+Shift+Z)': '重做被撤销的编辑（Ctrl+Shift+Z）',
  'The edit was put back': '已重做',
  'Nothing to put back': '没有可重做的编辑',
  'Show only what an annotation says. Enter applies, Escape clears.':
    '只显示某个注解说到的元素。回车应用，Esc 清除。',
  '{unit} is not in the unit table; comparisons stay undecided':
    '{unit} 不在单位表中，涉及它的比较保持未判定',
  'stale verdict': '判定已过期',
  detail: '理由',
  'judged by': '判定者',
  unknown: '未记录',
  'This verdict was computed against an older model. Re-judge it before relying on it.':
    '这条判定针对的是旧版模型。依赖它之前先重新判定。',
  verdicts: '判定',
  'stale verdicts': '过期判定',

  // ---- navigator -----------------------------------------------------
  Model: '模型',

  // ---- compartments and alignment ------------------------------------
  Compartments: '隔间',  'Show or hide the stereotype line': '显示或隐藏构造型行',
  'Show or hide the member compartment': '显示或隐藏成员隔间',
  'Show or hide the ports': '显示或隐藏端口',
  '{n} selected': '{n} 个已选',
  'Ctrl-click a box to add it to the selection.': '按住 Ctrl 点方框可加入选择。',
  Align: '对齐',
  'Align left': '左对齐',
  'Centre horizontally': '水平居中',
  'Align right': '右对齐',
  'Align top': '顶端对齐',
  'Centre vertically': '垂直居中',
  'Align bottom': '底端对齐',
  'Distribute horizontally': '水平等距',
  'Distribute vertically': '垂直等距',
  'Moves the boxes. The model text is not touched.': '只移动方框，不改模型文本。',

  // ---- bottom panel --------------------------------------------------
  Problems: '问题',
  Values: '值',
  'Open questions': '待答',
  Source: '源码',
  'No diagnostics.': '没有诊断。',
  'This model has no values.': '这个模型没有带值的属性。',
  'The ledger has no open questions.': '台账里没有提问。',
  Expand: '展开',
  Collapse: '收起',
  'Command palette': '命令面板',
  'Type a command': '输入命令',
  'No command matches': '没有匹配的命令',
  'Nothing to save': '没有要保存的改动',
  'Switch the model between compat and extended': '在 compat 和 extended 之间切换模型',
  'Command palette (Ctrl+Shift+P)': '命令面板 (Ctrl+Shift+P)',
  'Display': '显示',
  'Marks': '变更标记',
  'English': 'English',
  'This model has no problems.': '这个模型没有问题。',
  'Click to select the element this is about': '点击选中它所说的元素',
  'Show the problems': '显示问题',
};

/**
 * Translate one primitive and fill its `{placeholders}`.
 *
 * An unknown placeholder is left as written: a half-substituted label is a bug someone can see,
 * where an empty string reads as "this field does not exist".
 */
export function t(text: string, args?: Record<string, string | number>): string {
  const template = current === 'zh' ? (ZH[text] ?? text) : text;
  if (!args) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) => (
    name in args ? String(args[name]) : whole
  ));
}
