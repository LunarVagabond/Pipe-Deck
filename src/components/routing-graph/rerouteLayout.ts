export interface RerouteKnot {
  id: string;
  x: number;
  y: number;
}

const REROUTE_KEY = "pipe-deck-routing-reroutes";

type RerouteStore = Record<string, RerouteKnot[]>;

/** Stable cosmetic key for a wire between two graph nodes. */
export function rerouteEdgeKey(source: string, target: string): string {
  return `${source}|${target}`;
}

/** Reactive tick for Vue components reading reroute layout from localStorage. */
export const rerouteRevision = { value: 0 };

export function bumpRerouteRevision() {
  rerouteRevision.value += 1;
}

function loadStore(): RerouteStore {
  try {
    const raw = localStorage.getItem(REROUTE_KEY);
    return raw ? (JSON.parse(raw) as RerouteStore) : {};
  } catch {
    return {};
  }
}

function saveStore(store: RerouteStore) {
  localStorage.setItem(REROUTE_KEY, JSON.stringify(store));
  bumpRerouteRevision();
}

function resolveStoreKey(edgeKey: string): string {
  const store = loadStore();
  if (store[edgeKey]) {
    return edgeKey;
  }

  // Back-compat: older builds keyed reroutes by vue-flow edge id.
  const legacy = Object.keys(store).find((key) => key.includes(edgeKey) || edgeKey.includes(key));
  return legacy ?? edgeKey;
}

export function getReroutes(edgeKey: string): RerouteKnot[] {
  const store = loadStore();
  const key = resolveStoreKey(edgeKey);
  return store[key] ?? [];
}

export function setReroutes(edgeKey: string, knots: RerouteKnot[]) {
  const store = loadStore();
  const key = resolveStoreKey(edgeKey);
  if (knots.length === 0) {
    delete store[key];
  } else {
    store[key] = knots;
  }
  saveStore(store);
}

export function addReroute(edgeKey: string, x: number, y: number): RerouteKnot {
  const knot: RerouteKnot = {
    id: `reroute-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    x,
    y,
  };
  const next = [...getReroutes(edgeKey), knot];
  setReroutes(edgeKey, next);
  return knot;
}

export function updateReroute(edgeKey: string, knotId: string, x: number, y: number) {
  const next = getReroutes(edgeKey).map((knot) =>
    knot.id === knotId ? { ...knot, x, y } : knot,
  );
  setReroutes(edgeKey, next);
}

export function removeReroute(edgeKey: string, knotId: string) {
  const next = getReroutes(edgeKey).filter((knot) => knot.id !== knotId);
  setReroutes(edgeKey, next);
}

export function removeReroutesForEdge(edgeKey: string) {
  const store = loadStore();
  const key = resolveStoreKey(edgeKey);
  delete store[key];
  saveStore(store);
}

export function clearAllReroutes() {
  localStorage.removeItem(REROUTE_KEY);
  bumpRerouteRevision();
}

export function countAllReroutes(): number {
  const store = loadStore();
  return Object.values(store).reduce((total, knots) => total + knots.length, 0);
}
