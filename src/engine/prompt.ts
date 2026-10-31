// 给本地小模型的提示词和语法约束。
// 系统提示词和示例对话固定不变，放在最前面，llama-server 的 cache_prompt 能整段复用它们的 KV。

import { type Term, glossaryLine } from './glossary.ts'
import { type Graph, ICONS, MARKS, describe } from './graph.ts'
import { type SpokenLang, spokenLang } from './lang.ts'

export const SYSTEM = `你是会议板书员。大家在讨论，你根据「新的一句」更新白板。

白板上可以有两类东西，按讨论内容选：
- 流程图：讲流程、调用、数据流向、先后、因果时用。节点是名词，动作写在连线标签上。
- 要点板：列模块、分工、方案、待办、头脑风暴时用。先起一个话题，再往下挂条目，条目下还能挂子条目。
两类可以同时存在，也可以互相连线。

只输出操作，一行一条，不解释。甲乙写编号（n3）或「名字」：
E 甲 -> 乙 : 「标签」  连线，标签可省略，新名字会自动建节点
X 甲 -> 乙             删掉一条连线
G 「组名」 甲 乙        把节点放进同一组
T 「话题」             新起一个话题
L 甲 「条目」           在话题或条目下面加一条
Q 甲 「问题」           记一个还没定的问题，挂在甲旁边；不挂就写 Q 「问题」
F 甲 「公式」           记公式或短表达式；不挂就写 F 「公式」
K 甲 「备注」           在甲旁边写一句补充
M 甲 划掉|圈出|下划线|星标    [原文] 上可以再指定范围，见规则 10
I 甲 ${ICONS.join('|')}
H 甲                   只是指给大家看，高亮一下，板上不留字
R 甲 「新名字」         改名
A 甲 「别名」           记别名
D 甲                   删除（说错了、画错了）
-                      这句不需要改白板

规则：
1. 白板上已有的东西必须用编号引用，不要重复新建。「它」「这个」「那边」要对应到已有编号。这一轮刚新建的东西还没有编号，用「名字」引用。
2. 名字要短，用讨论里的叫法。节点 2 到 8 个字，条目可以到 16 个字。
3. 有先后、有走向的（先……再……、过了才、之后、然后、发到、调用、导致）一律用 E 画流程图，不要写成条目。
4. 一条只写一件事。一句话里列了几样，就拆成几条 L，每条 2 到 10 个字。
5. 「先说……」「第二块」「要改进的」「待办」这种分段，各起一个平级的 T 话题，不要套在别的话题下面。
6. 已有的条目换了归属（还是给小王、归到后端），用 L 新的上级 「这个条目」，它会被挪过去。
7. 连线画错了（不对、不是、改成）：X 掉错的线再加对的。方案、选项被否掉（不用了、划掉、先不做）：M 划掉，保留在板上。
8. 有人说「重点」「最关键」「一定要」：M 星标或圈出。带问号、谁来、要不要、还没想好的事：Q，不要写成条目。
9. 寒暄、跑题、情绪，或者只是复述白板上已有的内容，输出 -。句子里只要有白板上还没有的东西，就照常画，哪怕开头是「好」「那个」「总之」。
10. 实体表里标 [原文] 的是导入文件上的一段原文，位置固定：只能 M 标记、E 连线、K 写批注、Q 挂问题，不能改名、删除、往下挂条目。说到「这一段」「这里」「那张表」，或者念出原文里的一个词、一句话时，对应到全文里真有这些字的那块 [原文]。标记 [原文] 分三种范围：一个词 M 甲 圈出 「predicting」；一句话 M 甲 圈出 句 「这句里的几个词」；整段 M 甲 圈出。只念了几个词就只标这几个词；说了「这句」才用 句；说了「这段」「这一段」才标整段。原图上的线和箭头不在实体表里：要改原图上的某条线，就在线旁边那段 [原文] 上 M 划掉或 K 写批注，不要另画连线。和原文内容无关的事（分工、待办、别的话题）照常 T、L、E，不要挂到 [原文] 上。
11. 只是在带大家看哪里（我们先看摘要、去看引言、再看看这段、看这张图）：H 那块，不要写 K，也不要 M。这种话本身不是内容，不要抄到板上。
12. 分工按人起话题，事情挂在人下面。一件事是几个人一起做：把人名写在一个话题上（「小王、小李」），人名不要挂到人名下面。`

/** 示例用真正的多轮对话给出，小模型照着学比读规则可靠。 */
const EXAMPLES: [string, string][] = [
  [
    '白板：\n（空）\n新的一句：用户先打开小程序，小程序去调登录接口。',
    'E 「用户」 -> 「小程序」 : 「打开」\nE 「小程序」 -> 「登录接口」 : 「调用」',
  ],
  [
    '白板：\nn1 小程序\nn2 登录接口\nn1(小程序) -> n2(登录接口) 调用\n新的一句：不对，小程序得先过网关，网关再转给登录。',
    'X n1 -> n2\nE n1 -> 「网关」\nE 「网关」 -> n2 : 「转发」',
  ],
  [
    '白板：\nn1 订单服务\nn2 库存服务\nn1(订单服务) -> n2(库存服务)\n新的一句：它俩都放在交易域里吧，库存那边也叫 stock。',
    'G 「交易域」 n1 n2\nA n2 「stock」',
  ],
  ['白板：\nn1 订单服务\n新的一句：哈哈对，昨天那个会开太久了。', '-'],
  [
    '白板：\n（空）\n新的一句：爬虫这块要做三件事，监控任务、视频搜索、评论采集，监控是重点。',
    'T 「爬虫」\nL 「爬虫」 「监控任务」\nL 「爬虫」 「视频搜索」\nL 「爬虫」 「评论采集」\nM 「监控任务」 星标',
  ],
  [
    '白板：\nn1 [话题] 推送方案\n  n2 极光推送\n  n3 APNs 直推\n新的一句：极光就不用了，合同没续。那 MQ 要不要上还没想好。',
    'M n2 划掉\nK n2 「合同没续」\nQ n1 「要不要上 MQ」',
  ],
  [
    '白板：\nn1 [话题] 热度\nn2 graphRAG\n新的一句：热度就按现在时间减发布时间算，算出来的结果喂给 graphRAG。',
    'F n1 「T_now − T_publish」\nE n1 -> n2 : 「喂给」',
  ],
  [
    '白板：\n（空）\n新的一句：报销流程是先提交申请，主管审批过了才到财务打款。',
    'E 「提交申请」 -> 「主管审批」\nE 「主管审批」 -> 「财务打款」 : 「通过」',
  ],
  [
    '白板：\nn1 [话题] 做得好的\n  n2 按时上线\n新的一句：要改进的：联调太晚，还有需求老变。那周会要不要改成两次？',
    'T 「要改进的」\nL 「要改进的」 「联调太晚」\nL 「要改进的」 「需求老变」\nQ 「周会要不要改两次」',
  ],
  [
    '白板：\nn1 [话题] 小王\n  n2 登录页\nn3 [话题] 老李\n  n4 接口\n  n5 导出报表\n新的一句：导出报表还是给小王吧。',
    'L n1 「导出报表」',
  ],
  [
    '白板：\nn1 [话题] 小陈\n  n2 支付\n前文（已经画过了）：\nA：支付那块交给小陈来做。\n新的一句：其实还有老周，他俩一起。',
    'R n1 「小陈、老周」',
  ],
  [
    '白板：\nn1 [原文] Deep networks are hard to train. We present a residual framework that eases training.\n新的一句：residual 这个词圈一下。',
    'M n1 圈出 「residual」',
  ],
  [
    '白板：\nn1 [原文] Deep networks are hard to train. We present a residual framework that eases training.\n新的一句：deep networks are hard to train 这句话划个线。',
    'M n1 下划线 句 「Deep networks are hard」',
  ],
  [
    '白板：\nn1 [原文] Abstract\nn2 [原文] This work presents a decoupled framework for automation.\nn3 [原文] §1 Introduction\n新的一句：好，我们先看看引言。',
    'H n3',
  ],
]

export interface Utterance {
  speaker?: string
  text: string
}

function line(u: Utterance): string {
  return u.speaker ? `${u.speaker}：${u.text}` : u.text
}

/** Language the board should be written in, judged from the recent lines and the new one */
export function boardLang(recent: Utterance[], next: Utterance): SpokenLang {
  return spokenLang([...recent, next].map((u) => u.text))
}

/**
 * English meetings: the system prompt and examples are Chinese (kept fixed so the server can reuse
 * their KV cache), so say explicitly that the board stays in English. Chinese meetings get nothing
 * extra, so their prompt is exactly what the eval measured.
 */
const ENGLISH_BOARD = '这场会说的是英文：板上的名字、条目、问题、备注都用英文写，照原话的叫法，不要翻译成中文。名字 1 到 4 个词，条目最多 8 个词。'

export function userMessage(g: Graph, recent: Utterance[], next: Utterance, terms: Term[] = []): string {
  const parts = [`白板：\n${describe(g)}`]
  if (boardLang(recent, next) === 'en') parts.unshift(ENGLISH_BOARD)
  // 术语表放在最前面：一场会里不变，llama-server 的前缀缓存能复用
  if (terms.length) parts.unshift(`术语（起名照这个写法）：${glossaryLine(terms)}`)
  if (recent.length) parts.push(`前文（已经画过了）：\n${recent.map(line).join('\n')}`)
  parts.push(`新的一句：${line(next)}`)
  return parts.join('\n')
}

export function messages(g: Graph, recent: Utterance[], next: Utterance, terms: Term[] = []): { role: string; content: string }[] {
  return [
    { role: 'system', content: SYSTEM },
    ...EXAMPLES.flatMap(([u, a]) => [
      { role: 'user', content: u },
      { role: 'assistant', content: a },
    ]),
    { role: 'user', content: userMessage(g, recent, next, terms) },
  ]
}

const alt = (xs: readonly string[]) => xs.map((x) => `"${x}"`).join(' | ')

/**
 * 每次调用按当前白板生成 GBNF：编号只能从现有节点里选，模型就写不出不存在的 n99。
 */
export function grammar(g: Graph, maxOps = 8, lang: SpokenLang = 'zh'): string {
  // English names need more characters than Chinese ones for the same content
  const nameMax = lang === 'en' ? 48 : 20
  const ids = g.nodes.map((n) => `"${n.id}"`)
  // 长的放前面，避免 n1 抢先匹配 n12 的前缀
  ids.sort((a, b) => b.length - a.length)
  const ref = ids.length ? `ref ::= name | ${ids.join(' | ')}` : 'ref ::= name'
  return [
    `root ::= "-" | op ("\\n" op){0,${maxOps - 1}}`,
    'op ::= e | x | gr | t | l | q | f | k | m | i | h | r | a | d',
    'e ::= "E " ref " -> " ref (" : " name)?',
    'x ::= "X " ref " -> " ref',
    'gr ::= "G " name (" " ref){1,6}',
    't ::= "T " name',
    'l ::= "L " ref " " name',
    'q ::= "Q " (ref " ")? name',
    'f ::= "F " (ref " ")? name',
    'k ::= "K " ref " " name',
    `m ::= "M " ref " " (${alt(MARKS)}) (" " ("句 ")? quote)?`,
    'quote ::= "「" [^「」\\n]{1,40} "」"',
    `i ::= "I " ref " " (${alt(ICONS)})`,
    'h ::= "H " ref',
    'r ::= "R " ref " " name',
    'a ::= "A " ref " " name',
    'd ::= "D " ref',
    ref,
    `name ::= "「" [^「」\\n]{1,${nameMax}} "」"`,
  ].join('\n')
}
