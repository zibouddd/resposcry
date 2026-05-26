import { readCache } from "./cache";

export function selectGraph(seed: string[]) {
  return readCache(seed).filter((value) => value.length > 0);
}
