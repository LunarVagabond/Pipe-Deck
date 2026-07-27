import type { BuiltRoutingGraph } from "./buildGraph";

/**
 * BFS over the routing graph's edges treating each one as undirected —
 * isolating an effect node (#222) needs everything actually wired to it in
 * either direction (an upstream EQ feeding it, a downstream EQ it feeds
 * through a fan-out branch), not just what it forwards audio to.
 */
export function computeConnectedComponent(
  startId: string,
  edges: BuiltRoutingGraph["edges"],
): Set<string> {
  const adjacency = new Map<string, Set<string>>();
  function link(a: string, b: string) {
    if (!adjacency.has(a)) adjacency.set(a, new Set());
    adjacency.get(a)!.add(b);
  }
  for (const edge of edges) {
    link(edge.source, edge.target);
    link(edge.target, edge.source);
  }

  const visited = new Set<string>([startId]);
  const queue = [startId];
  while (queue.length > 0) {
    const current = queue.shift()!;
    for (const neighbor of adjacency.get(current) ?? []) {
      if (!visited.has(neighbor)) {
        visited.add(neighbor);
        queue.push(neighbor);
      }
    }
  }
  return visited;
}
