export function streamNodeId(streamId: string): string {
  return `stream:${streamId}`;
}

export function deviceNodeId(deviceId: string): string {
  return `device:${deviceId}`;
}

export function parseGraphNodeId(nodeId: string): { kind: "stream" | "device"; id: string } | null {
  const [kind, ...rest] = nodeId.split(":");
  if ((kind !== "stream" && kind !== "device") || rest.length === 0) {
    return null;
  }
  return { kind, id: rest.join(":") };
}
