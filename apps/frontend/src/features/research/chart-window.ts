export interface TimeWindow {
  from: number;
  to: number;
}
export function windowBounds(times: number[], window: TimeWindow | null): [number, number] {
  const full: [number, number] = [0, Math.max(0, times.length - 1)];
  if (!window || !times.length) return full;
  const start = times.findIndex((time) => time >= window.from);
  let end = -1;
  for (let i = times.length - 1; i >= 0; i--) {
    if (times[i] <= window.to) { end = i; break; }
  }
  return start < 0 || end < start ? full : [start, end];
}
export function moveChartWindow(
  times: number[],
  bounds: [number, number],
  action: "in" | "out" | "left" | "right",
): TimeWindow | null {
  if (!times.length) return null;
  const [start, end] = bounds,
    count = end - start + 1;
  let nextCount = count,
    nextStart = start;
  if (action === "in") nextCount = Math.max(2, Math.ceil(count / 2));
  if (action === "out") nextCount = Math.min(times.length, count * 2);
  if (action === "in" || action === "out") nextStart = end - nextCount + 1;
  if (action === "left") nextStart -= Math.max(1, Math.floor(count / 3));
  if (action === "right") nextStart += Math.max(1, Math.floor(count / 3));
  nextCount = Math.min(times.length, nextCount);
  nextStart = Math.max(0, Math.min(times.length - nextCount, nextStart));
  return nextCount === times.length
    ? null
    : { from: times[nextStart], to: times[nextStart + nextCount - 1] };
}
