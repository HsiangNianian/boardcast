// 线条、圈、框、箭头、小图标：都是近似的手画轨迹，交给同一支笔。

import { type Pt, type Stroke, densify, dist, polyLength } from './core'

/** 二次曲线：bend 是中点沿法线的偏移（像素） */
export function bow(a: Pt, b: Pt, bend: number, n = 18): Stroke {
  const len = dist(a, b) || 1
  const nx = -(b[1] - a[1]) / len
  const ny = (b[0] - a[0]) / len
  const c: Pt = [(a[0] + b[0]) / 2 + nx * bend * 2, (a[1] + b[1]) / 2 + ny * bend * 2]
  return quad(a, c, b, n)
}

export function quad(a: Pt, c: Pt, b: Pt, n = 18): Stroke {
  const out: Stroke = []
  for (let i = 0; i <= n; i++) {
    const t = i / n
    out.push([(1 - t) ** 2 * a[0] + 2 * (1 - t) * t * c[0] + t * t * b[0], (1 - t) ** 2 * a[1] + 2 * (1 - t) * t * c[1] + t * t * b[1]])
  }
  return out
}

export function cubic(a: Pt, c1: Pt, c2: Pt, b: Pt, n = 24): Stroke {
  const out: Stroke = []
  for (let i = 0; i <= n; i++) {
    const t = i / n
    const u = 1 - t
    out.push([
      u * u * u * a[0] + 3 * u * u * t * c1[0] + 3 * u * t * t * c2[0] + t * t * t * b[0],
      u * u * u * a[1] + 3 * u * u * t * c1[1] + 3 * u * t * t * c2[1] + t * t * t * b[1],
    ])
  }
  return out
}

/** 随手一条线：微微弯一点 */
export function line(a: Pt, b: Pt, r: () => number, sag = 0.02): Stroke {
  return bow(a, b, (r() - 0.5) * 2 * sag * dist(a, b), 8)
}

/** 圈：一圈多一点，起点终点错开，像真的随手圈起来 */
export function loop(
  cx: number,
  cy: number,
  rx: number,
  ry: number,
  r: () => number,
  o: { turns?: number; start?: number; tilt?: number; grow?: number; n?: number } = {},
): Stroke {
  const start = o.start ?? r() * Math.PI * 2
  const turns = o.turns ?? 1.08 + r() * 0.12
  const tilt = o.tilt ?? (r() - 0.5) * 0.12
  const grow = o.grow ?? 0.06
  const n = o.n ?? Math.max(36, Math.round(turns * 44))
  const ct = Math.cos(tilt)
  const st = Math.sin(tilt)
  const pts: Stroke = []
  for (let i = 0; i <= n; i++) {
    const t = start + (i / n) * turns * Math.PI * 2
    const g = 1 + (i / n) * grow
    const x = Math.cos(t) * rx * g
    const y = Math.sin(t) * ry * g
    pts.push([cx + x * ct - y * st, cy + x * st + y * ct])
  }
  return pts
}

/** 手画的框：四笔，拐角处略微出头 */
export function box(x: number, y: number, w: number, h: number, r: () => number, m = 0.3): Stroke[] {
  const k = 3 + 7 * m
  const o = () => (r() - 0.3) * k
  return [
    line([x + o(), y], [x + w + o(), y + o() * 0.3], r, 0.01),
    line([x + w, y + o()], [x + w + o() * 0.3, y + h + o()], r, 0.01),
    line([x + w + o(), y + h], [x + o(), y + h + o() * 0.3], r, 0.01),
    line([x, y + h + o()], [x + o() * 0.3, y + o()], r, 0.01),
  ]
}

/** 圆角框（虚线框用）：一圈闭合的轨迹 */
export function roundRect(x: number, y: number, w: number, h: number, rad: number, r: () => number): Stroke {
  const j = () => (r() - 0.5) * 2.4
  const R = Math.min(rad, w / 2, h / 2)
  const pts: Stroke = []
  const arc = (cx: number, cy: number, a0: number) => {
    for (let i = 0; i <= 5; i++) {
      const t = a0 + (i / 5) * (Math.PI / 2)
      pts.push([cx + Math.cos(t) * R, cy + Math.sin(t) * R])
    }
  }
  pts.push([x + R + j(), y + j()])
  pts.push([x + w - R + j(), y + j()])
  arc(x + w - R, y + R, -Math.PI / 2)
  pts.push([x + w + j(), y + h - R + j()])
  arc(x + w - R, y + h - R, 0)
  pts.push([x + R + j(), y + h + j()])
  arc(x + R, y + h - R, Math.PI / 2)
  pts.push([x + j(), y + R + j()])
  arc(x + R, y + R, Math.PI)
  pts.push([x + R + 6, y + j() * 0.5])
  return pts
}

/** 箭头的头：两笔 */
export function head(pts: Stroke, L: number, r: () => number, spread = 0.45): Stroke[] {
  const end = pts[pts.length - 1]
  // 往回找一个离尖端有点距离的点，方向更稳
  let k = pts.length - 2
  while (k > 0 && dist(pts[k], end) < L * 0.6) k--
  const prev = pts[Math.max(0, k)]
  const ang = Math.atan2(end[1] - prev[1], end[0] - prev[0])
  const sp = spread + r() * 0.15
  const one = (s: number): Stroke => [[end[0] - Math.cos(ang + s) * L, end[1] - Math.sin(ang + s) * L], end]
  return [one(sp), one(-sp)]
}

export function arrow(pts: Stroke, L: number, r: () => number): Stroke[] {
  return [pts, ...head(pts, L, r)]
}

/** 切成一段段虚线 */
export function dashes(pts: Stroke, dash: number, gap: number): Stroke[] {
  const dense = densify(pts, 2)
  const out: Stroke[] = []
  let cur: Stroke = []
  let acc = 0
  let on = true
  for (let i = 0; i < dense.length; i++) {
    if (i) acc += dist(dense[i - 1], dense[i])
    if (on) cur.push(dense[i])
    if (on && acc >= dash) {
      if (cur.length > 1) out.push(cur)
      cur = []
      on = false
      acc = 0
    } else if (!on && acc >= gap) {
      on = true
      acc = 0
      cur = [dense[i]]
    }
  }
  if (on && cur.length > 1 && polyLength(cur) > dash * 0.3) out.push(cur)
  return out
}

/** 一道划掉的线 */
export function strike(x: number, y: number, w: number, tilt: number, r: () => number): Stroke {
  return line([x - 6, y + tilt], [x + w + 8, y - tilt], r, 0.025)
}

/** 小波浪线（穿过字缝的细枝用） */
export function wave(a: Pt, b: Pt, amp: number, waves: number, n = 60): Stroke {
  const len = dist(a, b) || 1
  const nx = -(b[1] - a[1]) / len
  const ny = (b[0] - a[0]) / len
  const out: Stroke = []
  for (let i = 0; i <= n; i++) {
    const t = i / n
    const s = Math.sin(t * Math.PI * 2 * waves) * amp * Math.sin(t * Math.PI) ** 0.4
    out.push([a[0] + (b[0] - a[0]) * t + nx * s, a[1] + (b[1] - a[1]) * t + ny * s])
  }
  return out
}

export type Icon = 'phone' | 'person' | 'db' | 'doc' | 'cloud' | 'chart' | 'chat' | 'mic' | 'cpu'

function oval(cx: number, cy: number, rx: number, ry: number, r: () => number): Stroke {
  const start = r() * Math.PI * 2
  const pts: Stroke = []
  for (let i = 0; i <= 28; i++) {
    const t = start + (i / 28) * Math.PI * 2.04
    pts.push([cx + Math.cos(t) * rx, cy + Math.sin(t) * ry])
  }
  return pts
}

/** 小图标：和桌面版同一套造型。坐标以边长为 1，(x, y) 是左上角 */
export function icon(kind: Icon, x: number, y: number, s: number, r: () => number): Stroke[] {
  const P = (a: number, b: number): Pt => [x + a * s, y + b * s]
  const seg = (a: [number, number], b: [number, number]) => line(P(...a), P(...b), r, 0.03)
  const poly = (pts: [number, number][]): Stroke => pts.map(([a, b]) => P(a, b))
  const rect = (a: number, b: number, w: number, h: number): Stroke =>
    poly([
      [a, b],
      [a + w, b],
      [a + w, b + h],
      [a, b + h],
      [a, b + 0.02],
    ])
  const ell = (a: number, b: number, w: number, h: number) => oval(x + (a + w / 2) * s, y + (b + h / 2) * s, (w / 2) * s, (h / 2) * s, r)
  const arc = (x0: number, yy: number, x1: number, h: number): [number, number][] => {
    const out: [number, number][] = []
    for (let i = 0; i <= 12; i++) {
      const t = Math.PI * (i / 12)
      out.push([x0 + ((x1 - x0) / 2) * (1 - Math.cos(t)), yy + Math.sin(t) * h])
    }
    return out
  }
  switch (kind) {
    case 'phone':
      return [rect(0.22, 0, 0.56, 1), seg([0.42, 0.88], [0.58, 0.88])]
    case 'person':
      return [
        ell(0.3, 0, 0.4, 0.4),
        poly([
          [0.08, 1],
          [0.2, 0.62],
          [0.5, 0.5],
          [0.8, 0.62],
          [0.92, 1],
        ]),
      ]
    case 'db':
      return [ell(0.1, 0, 0.8, 0.28), seg([0.1, 0.14], [0.1, 0.86]), seg([0.9, 0.14], [0.9, 0.86]), poly(arc(0.1, 0.86, 0.9, 0.14))]
    case 'doc':
      return [
        poly([
          [0.15, 0],
          [0.65, 0],
          [0.85, 0.2],
          [0.85, 1],
          [0.15, 1],
          [0.15, 0],
        ]),
        seg([0.3, 0.45], [0.7, 0.45]),
        seg([0.3, 0.65], [0.7, 0.65]),
      ]
    case 'cloud':
      return [ell(0, 0.35, 0.5, 0.45), ell(0.25, 0.1, 0.5, 0.55), ell(0.5, 0.35, 0.5, 0.45)]
    case 'chart':
      return [
        poly([
          [0.05, 0],
          [0.05, 0.95],
          [1, 0.95],
        ]),
        rect(0.2, 0.55, 0.15, 0.4),
        rect(0.45, 0.3, 0.15, 0.65),
        rect(0.7, 0.45, 0.15, 0.5),
      ]
    case 'chat':
      return [
        poly([
          [0, 0.1],
          [1, 0.1],
          [1, 0.7],
          [0.4, 0.7],
          [0.2, 0.95],
          [0.25, 0.7],
          [0, 0.7],
          [0, 0.1],
        ]),
      ]
    case 'mic':
      return [rect(0.35, 0, 0.3, 0.58), poly(arc(0.2, 0.42, 0.8, 0.22)), seg([0.5, 0.66], [0.5, 0.92]), seg([0.3, 0.95], [0.7, 0.95])]
    case 'cpu':
      return [
        rect(0.2, 0.2, 0.6, 0.6),
        seg([0.35, 0.05], [0.35, 0.2]),
        seg([0.65, 0.05], [0.65, 0.2]),
        seg([0.35, 0.8], [0.35, 0.95]),
        seg([0.65, 0.8], [0.65, 0.95]),
        seg([0.05, 0.4], [0.2, 0.4]),
        seg([0.8, 0.6], [0.95, 0.6]),
      ]
  }
}
