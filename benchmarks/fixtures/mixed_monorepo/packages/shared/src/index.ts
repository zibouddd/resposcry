export function read_cache(seed: string[]) {
  return seed.map((value, index) => `${value}:${index}`);
}

export function rebuild_graph(seed: string[]) {
  return seed.map((value, index) => ({ id: value, edges: index + 1 }));
}
