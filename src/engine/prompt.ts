// 给本地小模型的提示词和语法约束。
// 系统提示词和示例对话固定不变，放在最前面，llama-server 的 cache_prompt 能整段复用它们的 KV。

import { type Graph, describe } from './graph.ts'

export const SYSTEM = `你是会议板书员。大家在讨论，你根据「新的一句」更新白板上的示意图。

白板是一张图：
- 节点是名词：系统、模块、角色、数据、步骤、事件。
- 连线是关系：流向、调用、先后、因果。动作（打开、调用、转给、导致）写在连线的标签上，不要当成节点。

只输出操作，一行一条，不解释：
E 甲 -> 乙 : 「标签」  从甲连到乙，标签可省略。甲乙写编号（n3）或「新名字」，新名字会自动建节点
N 「名字」             只新增节点，不连线
X 甲 -> 乙             删掉甲到乙的连线
R 甲 「新名字」         改名
A 甲 「别名」           记别名
G 「组名」 甲 乙        把节点放进同一组
D 甲                   删除节点
-                      这句不需要改白板

规则：
1. 白板上已有的东西必须用编号引用，不要重复新建。「它」「这个」「那边」要对应到已有编号。
2. 名字 2 到 8 个字，用讨论里的叫法，不要带动词和语气词。
3. 有人纠正（不对、不是、改成），先 X 掉错的那条线，再加对的。整个东西不要了（划掉、不用了、去掉），用 D 删节点。
4. 寒暄、跑题、情绪、提问、还没定的猜测，输出 -。
5. 总结、强调、重复白板上已有的内容（所以、总之、根因还是），不是纠正，输出 -。`

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
    '白板：\nn1 配置中心\nn2 网关超时\nn1(配置中心) -> n2(网关超时) 导致\n新的一句：网关一超时，客户端就疯狂重试，流量翻了三倍。',
    'E n2 -> 「客户端重试」 : 「触发」\nE 「客户端重试」 -> 「流量升高」',
  ],
  ['白板：\nn1 极光推送\nn2 手机\nn1(极光推送) -> n2(手机)\n新的一句：极光那个方案不用了，划掉。', 'D n1'],
]

export interface Utterance {
  speaker?: string
  text: string
}

function line(u: Utterance): string {
  return u.speaker ? `${u.speaker}：${u.text}` : u.text
}

export function userMessage(g: Graph, recent: Utterance[], next: Utterance): string {
  const parts = [`白板：\n${describe(g)}`]
  if (recent.length) parts.push(`前文（已经画过了）：\n${recent.map(line).join('\n')}`)
  parts.push(`新的一句：${line(next)}`)
  return parts.join('\n')
}

export function messages(g: Graph, recent: Utterance[], next: Utterance): { role: string; content: string }[] {
  return [
    { role: 'system', content: SYSTEM },
    ...EXAMPLES.flatMap(([u, a]) => [
      { role: 'user', content: u },
      { role: 'assistant', content: a },
    ]),
    { role: 'user', content: userMessage(g, recent, next) },
  ]
}

/**
 * 每次调用按当前白板生成 GBNF：编号只能从现有节点里选，模型就写不出不存在的 n99。
 */
export function grammar(g: Graph, maxOps = 6): string {
  const ids = g.nodes.map((n) => `"${n.id}"`)
  // 长的放前面，避免 n1 抢先匹配 n12 的前缀
  ids.sort((a, b) => b.length - a.length)
  const ref = ids.length ? `ref ::= name | ${ids.join(' | ')}` : 'ref ::= name'
  return [
    `root ::= "-" | op ("\\n" op){0,${maxOps - 1}}`,
    'op ::= e | n | x | r | a | gr | d',
    'e ::= "E " ref " -> " ref (" : " name)?',
    'n ::= "N " name',
    'x ::= "X " ref " -> " ref',
    'r ::= "R " ref " " name',
    'a ::= "A " ref " " name',
    'gr ::= "G " name (" " ref){1,6}',
    'd ::= "D " ref',
    ref,
    'name ::= "「" [^「」\\n ]{1,12} "」"',
  ].join('\n')
}
