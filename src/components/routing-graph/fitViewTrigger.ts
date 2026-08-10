export interface TrackedNode {
  id: string;
  identityKey?: string;
}

export interface NewNodeDetectionResult {
  addedIds: string[];
  nodeIds: Set<string>;
  identityKeys: Set<string>;
}

/**
 * Decides which of `current`'s nodes are genuinely new, to drive the
 * fitView-on-new-node behavior. A stream's underlying PipeWire node can be
 * torn down and recreated with a new node id for reasons unrelated to the
 * user adding a new source (e.g. Firefox recreating its audio node when a
 * tab's playback pauses/resumes) — `identityKey` (app_name/executable/
 * media_name, stable across that churn) is used to filter those
 * reappearances out, so only truly new nodes trigger a re-center/zoom.
 *
 * The very first call (`knownNodeIds === null`) always reports no additions
 * — there's nothing to compare against yet, and fitView shouldn't fire on
 * initial mount.
 */
export function detectNewlyAddedNodes(
  current: TrackedNode[],
  knownNodeIds: Set<string> | null,
  knownStreamIdentityKeys: Set<string> | null,
): NewNodeDetectionResult {
  const currentIds = new Set(current.map((node) => node.id));
  const currentIdentityKeys = new Set(
    current
      .map((node) => node.identityKey)
      .filter((key): key is string => !!key),
  );

  if (knownNodeIds === null) {
    return {
      addedIds: [],
      nodeIds: currentIds,
      identityKeys: currentIdentityKeys,
    };
  }

  const addedIds = [...currentIds].filter((id) => {
    if (knownNodeIds.has(id)) return false;
    const identityKey = current.find((node) => node.id === id)?.identityKey;
    if (identityKey && knownStreamIdentityKeys?.has(identityKey)) return false;
    return true;
  });

  return { addedIds, nodeIds: currentIds, identityKeys: currentIdentityKeys };
}
