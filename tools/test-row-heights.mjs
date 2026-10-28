import assert from "node:assert/strict";
import test from "node:test";
import { RowHeights } from "../src/features/diff/rowHeights.ts";

const UNIT = 19;

test("不换行时保持固定行高与直接乘除的位置", () => {
  const heights = new RowHeights(200, UNIT, false);
  assert.equal(heights.total(), 200 * UNIT);
  assert.equal(heights.measure(5, UNIT * 3), false);
  for (const position of [0, 1, 1.5, 20.25, 199.75, 200]) {
    assert.equal(heights.top(position), Math.floor(position) * UNIT);
    assert.equal(heights.yAt(position), position * UNIT);
    assert.equal(heights.rowAtFraction(position * UNIT), position);
    assert.equal(heights.rowAt(position * UNIT), Math.min(199, Math.floor(position)));
  }
});

test("测量变高和变矮都更新前缀位置，重复测量不失效", () => {
  const heights = new RowHeights(4, UNIT, true);
  assert.equal(heights.total(), 4 * UNIT);
  assert.equal(heights.measure(1, 3 * UNIT), true);
  assert.equal(heights.height(1), 3 * UNIT);
  assert.equal(heights.top(2), 4 * UNIT);
  assert.equal(heights.total(), 6 * UNIT);
  assert.equal(heights.rowAt(4 * UNIT - 0.1), 1);
  assert.equal(heights.rowAt(4 * UNIT), 2);
  assert.equal(heights.measure(1, 3 * UNIT), false);
  assert.equal(heights.measure(1, UNIT), true);
  assert.equal(heights.top(2), 2 * UNIT);
  assert.equal(heights.total(), 4 * UNIT);
});

test("变高的行内比例与像素位置双向互换", () => {
  const heights = new RowHeights(4, UNIT, true);
  heights.measure(1, 2 * UNIT);
  heights.measure(2, 4 * UNIT);
  for (const position of [0, 0.5, 1, 1.125, 1.75, 2, 2.5, 2.875, 3, 3.5, 4]) {
    assert.equal(heights.rowAtFraction(heights.yAt(position)), position);
  }
  assert.equal(heights.yAt(1.5), 2 * UNIT);
  assert.equal(heights.yAt(2.5), 5 * UNIT);
});

test("前方行和锚点行高度改变后仍可恢复同一逻辑位置", () => {
  const heights = new RowHeights(6, UNIT, true);
  heights.measure(2, 4 * UNIT);
  const anchor = heights.rowAtFraction(heights.yAt(2.375));
  const previousY = heights.yAt(anchor);
  heights.measure(0, 3 * UNIT);
  heights.measure(1, 2 * UNIT);
  heights.measure(2, 2 * UNIT);
  const nextY = heights.yAt(anchor);
  assert.notEqual(nextY, previousY);
  assert.equal(heights.rowAtFraction(nextY), 2.375);
});

test("固定和换行模式都夹紧首尾，空集合始终返回零位置", () => {
  for (const wrap of [false, true]) {
    const heights = new RowHeights(4, UNIT, wrap);
    heights.measure(0, 2 * UNIT);
    for (const value of [-Infinity, -3, Number.NaN]) {
      assert.equal(heights.top(value), 0);
      assert.equal(heights.rowAt(value), 0);
      assert.equal(heights.rowAtFraction(value), 0);
      assert.equal(heights.yAt(value), 0);
    }
    for (const value of [1000, Infinity]) {
      assert.equal(heights.top(value), heights.total());
      assert.equal(heights.rowAt(value), 3);
      assert.equal(heights.rowAtFraction(value), 4);
      assert.equal(heights.yAt(value), heights.total());
    }
    const empty = new RowHeights(0, UNIT, wrap);
    for (const value of [-Infinity, -1, 0, 0.5, 1000, Infinity, Number.NaN]) {
      assert.equal(empty.top(value), 0);
      assert.equal(empty.rowAt(value), 0);
      assert.equal(empty.rowAtFraction(value), 0);
      assert.equal(empty.yAt(value), 0);
    }
    assert.equal(empty.total(), 0);
    assert.equal(empty.measure(0, UNIT), false);
  }
});

test("忽略无效测量，并限制行数避免 Uint16 高度溢出", () => {
  const heights = new RowHeights(2, UNIT, true);
  for (const row of [-1, 0.5, 2, Infinity, Number.NaN]) {
    assert.equal(heights.measure(row, 2 * UNIT), false);
  }
  for (const pixels of [-1, Infinity, Number.NaN]) {
    assert.equal(heights.measure(0, pixels), false);
  }
  assert.equal(heights.measure(0, 0), false);
  assert.equal(heights.measure(0, 70000 * UNIT), true);
  assert.equal(heights.height(0), 0xffff * UNIT);
  assert.equal(heights.total(), 0x10000 * UNIT);
});
