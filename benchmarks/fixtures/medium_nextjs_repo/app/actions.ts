"use server";

import { rebuild_graph } from "@/lib/graph";

export async function refreshGraph() {
  return rebuild_graph(["route", "action", "page"]);
}
