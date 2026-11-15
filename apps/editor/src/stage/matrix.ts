// 2D affine math for the stage. A matrix is `[a, b, c, d, tx, ty]`, the runtime's layout:
// x' = a*x + c*y + tx, y' = b*x + d*y + ty. Framework-free.

export type Mat = readonly [number, number, number, number, number, number];
export type Point = readonly [x: number, y: number];

export const IDENTITY: Mat = [1, 0, 0, 1, 0, 0];

export const apply = (m: Mat, x: number, y: number): Point => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
/** Transform a direction (ignores the translation). */
export const applyLinear = (m: Mat, x: number, y: number): Point => [m[0] * x + m[2] * y, m[1] * x + m[3] * y];

/** The inverse, or null when the matrix is singular (a zero scale). */
export function invert(m: Mat): Mat | null {
  const det = m[0] * m[3] - m[1] * m[2];
  if (Math.abs(det) < 1e-12) return null;
  const [a, b, c, d] = [m[3] / det, -m[1] / det, -m[2] / det, m[0] / det];
  return [a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])];
}
