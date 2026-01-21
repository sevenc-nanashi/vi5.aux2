export function colorToP5Tuple(color: {
  r: number;
  g: number;
  b: number;
  a: number;
}): [number, number, number, number] {
  return [color.r * 255, color.g * 255, color.b * 255, color.a * 255];
}
