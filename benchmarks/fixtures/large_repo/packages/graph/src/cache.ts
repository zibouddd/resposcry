export function readCache(seed: string[]) {
  return seed.map((value, index) => `${value}:${index}`);
}
