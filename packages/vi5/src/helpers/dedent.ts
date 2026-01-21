export function dedent(string: string): string {
  const lines = string.split("\n");
  let minIndent: number | null = null;

  for (const line of lines) {
    const match = line.match(/^(\s*)\S/);
    if (match) {
      const indent = match[1]!.length;
      if (minIndent === null || indent < minIndent) {
        minIndent = indent;
      }
    }
  }
  if (minIndent !== null && minIndent > 0) {
    return lines
      .map((line) => (line.startsWith(" ".repeat(minIndent)) ? line.slice(minIndent) : line))
      .join("\n");
  }
  return string;
}
