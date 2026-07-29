import type { BuiltRoutingGraph } from "./buildGraph";

/**
 * Directed BFS over the routing graph's edges (issue #223) — latency is a
 * property of actual signal flow, so unlike `computeConnectedComponent`'s
 * undirected traversal, this only follows edges in the direction they're
 * drawn (`source -> target`) and returns the ordered node-id path rather
 * than just the reachable set.
 */
export function findNodePath(
  sourceNodeId: string,
  targetNodeId: string,
  edges: BuiltRoutingGraph["edges"],
): string[] | null {
  if (sourceNodeId === targetNodeId) {
    return [sourceNodeId];
  }

  const adjacency = new Map<string, string[]>();
  for (const edge of edges) {
    if (!adjacency.has(edge.source)) adjacency.set(edge.source, []);
    adjacency.get(edge.source)!.push(edge.target);
  }

  const predecessor = new Map<string, string>();
  const visited = new Set<string>([sourceNodeId]);
  const queue = [sourceNodeId];

  while (queue.length > 0) {
    const current = queue.shift()!;
    for (const neighbor of adjacency.get(current) ?? []) {
      if (visited.has(neighbor)) continue;
      visited.add(neighbor);
      predecessor.set(neighbor, current);
      if (neighbor === targetNodeId) {
        const path = [targetNodeId];
        let node = targetNodeId;
        while (node !== sourceNodeId) {
          node = predecessor.get(node)!;
          path.unshift(node);
        }
        return path;
      }
      queue.push(neighbor);
    }
  }

  return null;
}
