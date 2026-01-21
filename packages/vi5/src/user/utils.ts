export function colorToP5Tuple(color: {
  r: number;
  g: number;
  b: number;
  a: number;
}): [number, number, number, number] {
  return [color.r, color.g, color.b, color.a];
}
