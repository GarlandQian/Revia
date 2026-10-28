/**
 * 显示行的纵向位置（软换行，Q139）。
 *
 * 不换行时每行等高，位置直接按下标乘行高，与以前完全相同（D06 的测量路径不变）。
 * 软换行时一行可能折成几行：折成几行由渲染后量出的高度给出，没量过的行按一行算；
 * 位置用前缀和求，按位置找行用二分。显示行可达数十万，前缀和只在有行的高度
 * 变了之后、下一次查询时重算一次。
 */

export class RowHeights {
  readonly count: number;
  readonly unit: number;
  readonly wrap: boolean;
  /** 每行折成几行（只在软换行时分配）。 */
  private readonly lines: Uint16Array | null;
  /** `offsets[i]` 是第 i 行之前一共有几行（单位：行高），长度 count + 1。 */
  private offsets: Float64Array | null = null;

  constructor(count: number, unit: number, wrap: boolean) {
    this.count = Number.isFinite(count) ? Math.max(0, Math.floor(count)) : 0;
    this.unit = Number.isFinite(unit) && unit > 0 ? unit : 1;
    this.wrap = wrap;
    this.lines = wrap ? new Uint16Array(this.count).fill(1) : null;
  }

  /** 第 row 行的顶部（像素）；超出末行时给出内容总高。 */
  top(row: number): number {
    const at = bounded(Math.floor(row), this.count);
    if (!this.lines) return at * this.unit;
    return this.prefix()[at] * this.unit;
  }

  /** 第 row 行的高度（像素）。 */
  height(row: number): number {
    const at = Math.floor(row);
    if (!this.lines || !Number.isFinite(at) || at < 0 || at >= this.count) return this.unit;
    return this.lines[at] * this.unit;
  }

  total(): number {
    return this.top(this.count);
  }

  /** 位置 y 落在哪一行（夹在有效范围内；没有行时为 0）。 */
  rowAt(y: number): number {
    if (this.count === 0) return 0;
    if (!this.lines) return bounded(Math.floor(y / this.unit), this.count - 1);
    const offsets = this.prefix();
    const target = Number.isNaN(y) ? 0 : y / this.unit;
    let low = 0;
    let high = this.count - 1;
    let found = 0;
    while (low <= high) {
      const mid = (low + high) >> 1;
      if (offsets[mid] <= target) {
        found = mid;
        low = mid + 1;
      } else {
        high = mid - 1;
      }
    }
    return found;
  }

  /** 位置 y 换成“第几行 + 行内比例”；夹在 [0, count]，count 表示内容末尾。 */
  rowAtFraction(y: number): number {
    if (this.count === 0) return 0;
    if (!this.lines) return bounded(y / this.unit, this.count);
    const position = bounded(y, this.total());
    if (position === this.total()) return this.count;
    const row = this.rowAt(position);
    return row + (position - this.top(row)) / this.height(row);
  }

  /** `rowAtFraction` 在有效范围内的反函数；越界时夹到内容首尾。 */
  yAt(rowFraction: number): number {
    const position = bounded(rowFraction, this.count);
    if (!this.lines) return position * this.unit;
    const row = Math.floor(position);
    return this.top(row) + (position - row) * this.height(row);
  }

  /** 记下量出来的高度（像素，按行高取整）；高度变了返回 true。 */
  measure(row: number, pixels: number): boolean {
    if (!this.lines || !Number.isInteger(row) || row < 0 || row >= this.count
      || !Number.isFinite(pixels) || pixels < 0) return false;
    const lines = Math.min(0xffff, Math.max(1, Math.round(pixels / this.unit)));
    if (this.lines[row] === lines) return false;
    this.lines[row] = lines;
    this.offsets = null;
    return true;
  }

  private prefix(): Float64Array {
    if (this.offsets) return this.offsets;
    const lines = this.lines!;
    const offsets = new Float64Array(this.count + 1);
    for (let index = 0; index < this.count; index += 1) offsets[index + 1] = offsets[index] + lines[index];
    this.offsets = offsets;
    return offsets;
  }
}

function bounded(value: number, max: number): number {
  return Math.min(max, Math.max(0, Number.isNaN(value) ? 0 : value));
}
