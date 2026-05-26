export function rebuild_graph(seed: string[]) {
  return seed.map((value, index) => ({
    id: `${value}-${index}`,
    edges: index + 1,
    weight: index * 10,
  }));
}
