import { expect, it } from "vitest";
import { moveChartWindow, windowBounds } from "./chart-window";
it("zooms and pans within available bars and resets to the full dataset", () => {
  const times = [10, 20, 30, 40, 50, 60, 70, 80];
  const zoom = moveChartWindow(times, [0, 7], "in");
  expect(windowBounds(times, zoom)).toEqual([4, 7]);
  const earlier = moveChartWindow(times, [4, 7], "left");
  expect(windowBounds(times, earlier)).toEqual([3, 6]);
  expect(windowBounds(times, moveChartWindow(times, [4, 7], "right"))).toEqual([4, 7]);
  expect(moveChartWindow(times, [3, 6], "out")).toBeNull();
  expect(moveChartWindow([], [0, 0], "in")).toBeNull();
});
it("keeps the selected timestamps when refreshed data append or drop a bar", () => {
  const selection = { from: 30, to: 50 };
  expect(windowBounds([10, 20, 30, 40, 50, 60], selection)).toEqual([2, 4]);
  expect(windowBounds([20, 30, 40, 50, 60, 70], selection)).toEqual([1, 3]);
  expect(windowBounds([60, 70], selection)).toEqual([0, 1]);
  expect(windowBounds([10], null)).toEqual([0, 0]);
});
