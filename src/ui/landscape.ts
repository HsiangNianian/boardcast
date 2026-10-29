// 手机竖着拿时，style.css 把整个应用顺时针转了 90 度来横屏显示。
// 这时候指针事件的 clientX/clientY 还是屏幕坐标，自己算位置的地方要先换回应用里的坐标。

const QUERY = '(orientation: portrait) and (pointer: coarse) and (max-width: 900px)'

export function forcedLandscape(): boolean {
  return matchMedia(QUERY).matches
}

/** 屏幕坐标 → 元素内部（转之前）的坐标 */
export function localPoint(el: Element, clientX: number, clientY: number): [number, number] {
  const r = el.getBoundingClientRect()
  if (!forcedLandscape()) return [clientX - r.left, clientY - r.top]
  // 顺时针转 90 度：元素的 x 轴朝屏幕下方，y 轴朝屏幕左边
  return [clientY - r.top, r.right - clientX]
}

/** 能锁就真的锁成横屏（安卓上全屏以后才行），锁不了就靠上面的旋转 */
export function tryLockLandscape() {
  const o = screen.orientation as ScreenOrientation & { lock?: (o: string) => Promise<void> }
  void o?.lock?.('landscape').catch(() => undefined)
}
