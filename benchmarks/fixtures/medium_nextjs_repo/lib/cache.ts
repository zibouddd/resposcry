export function loadCache(seed: string[]) {
  return seed.map((value, index) => `${value}:${index}`);
}
