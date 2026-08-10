import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  makeDevice,
  makeGraph,
  makeProcessingNode,
  makeStream,
} from "../../test/graphFixtures";
import {
  buildRoutingGraph,
  deviceNodeId,
  processingNodeNodeId,
  streamNodeId,
} from "./buildGraph";
import type { RoutingGraphNodeData } from "./buildGraph";
import type { ActionStatus } from "../../types/graph";

// Node's own global `localStorage` (unrelated to jsdom's) takes precedence
// in this Vitest/Node combo and has no-op storage methods, so
// `buildRoutingGraph`'s layout persistence silently breaks unless a real
// backing store is stubbed in for the test.
function stubLocalStorage() {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => store.set(key, value),
    removeItem: (key: string) => store.delete(key),
    clear: () => store.clear(),
  });
}

describe("streamNodeKind route warnings", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  function dataFor(
    action_status: ActionStatus | undefined,
  ): RoutingGraphNodeData | undefined {
    const stream = makeStream({
      id: "s1",
      route_explanation: action_status
        ? {
            source: "authored_rule",
            match_reasons: [],
            skipped_candidates: [],
            action_status,
            fallback_applied: false,
          }
        : undefined,
    });
    const graph = makeGraph([], [stream]);
    const node = buildRoutingGraph(graph).nodes.find(
      (n) => n.id === streamNodeId("s1"),
    );
    return node?.data as RoutingGraphNodeData | undefined;
  }

  it("has no warning when there is no route explanation", () => {
    expect(dataFor(undefined)?.routeWarning).toBeUndefined();
  });

  it.each<[ActionStatus, "blocked" | "unavailable"]>([
    ["blocked", "blocked"],
    ["skipped_manual_override", "blocked"],
    ["target_unavailable", "unavailable"],
  ])("maps action_status %s to routeWarning %s", (status, expected) => {
    const data = dataFor(status);
    expect(data?.routeWarning).toBe(expected);
    expect(data?.routeWarningTitle).toBeTruthy();
  });

  it.each<ActionStatus>(["applied", "simulated", "no_action"])(
    "has no warning for action_status %s",
    (status) => {
      expect(dataFor(status)?.routeWarning).toBeUndefined();
    },
  );
});

describe("streamNodeKind format-mismatch badge (#156)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  function dataForFormats(
    stream: { sample_rate?: number; channels?: number },
    device: { sample_rate?: number; channels?: number },
  ): RoutingGraphNodeData | undefined {
    const target = makeDevice({ id: "d1", ...device });
    const s = makeStream({ id: "s1", current_target: "d1", ...stream });
    const graph = makeGraph([target], [s]);
    const node = buildRoutingGraph(graph).nodes.find(
      (n) => n.id === streamNodeId("s1"),
    );
    return node?.data as RoutingGraphNodeData | undefined;
  }

  it("has no badge when rate and channels match", () => {
    const data = dataForFormats(
      { sample_rate: 48000, channels: 2 },
      { sample_rate: 48000, channels: 2 },
    );
    expect(data?.formatMismatch).toBeUndefined();
  });

  it("shows a badge with the rate in the title when sample rate differs", () => {
    const data = dataForFormats(
      { sample_rate: 44100, channels: 2 },
      { sample_rate: 48000, channels: 2 },
    );
    expect(data?.formatMismatch).toBe(true);
    expect(data?.formatMismatchTitle).toContain("44100 Hz → 48000 Hz");
  });

  it("shows a badge when channel count differs", () => {
    const data = dataForFormats({ channels: 1 }, { channels: 2 });
    expect(data?.formatMismatch).toBe(true);
  });

  it("has no badge when either side's format is unknown (not a false positive)", () => {
    const data = dataForFormats({ sample_rate: 44100 }, {});
    expect(data?.formatMismatch).toBeUndefined();
  });
});

describe("stream layout position survives a node-id change (pause/resume jump fix)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("carries a stream's saved position onto its replacement when the identity is unambiguous", () => {
    const firstStream = makeStream({
      id: "node-1",
      app_name: "Firefox",
      executable: "firefox",
      media_name: undefined,
    });
    const firstGraph = makeGraph([], [firstStream]);
    const firstBuild = buildRoutingGraph(firstGraph);
    const originalPosition = firstBuild.nodes.find(
      (n) => n.id === streamNodeId("node-1"),
    )!.position;

    // Simulate the PipeWire node being torn down and recreated (e.g. Firefox
    // on tab pause/resume) as a new node id, same app_name/executable.
    const replacementStream = makeStream({
      id: "node-2",
      app_name: "Firefox",
      executable: "firefox",
      media_name: undefined,
    });
    const secondGraph = makeGraph([], [replacementStream]);
    const secondBuild = buildRoutingGraph(secondGraph);
    const newPosition = secondBuild.nodes.find(
      (n) => n.id === streamNodeId("node-2"),
    )!.position;

    expect(newPosition).toEqual(originalPosition);
  });

  it("does not migrate a position when the identity is ambiguous (two simultaneous same-identity streams)", () => {
    const firstStream = makeStream({
      id: "node-1",
      app_name: "Firefox",
      executable: "firefox",
    });
    const firstGraph = makeGraph([], [firstStream]);
    buildRoutingGraph(firstGraph);

    // Two simultaneous streams now share node-1's identity — ambiguous, so
    // neither should blindly inherit node-1's old position.
    const replacementA = makeStream({
      id: "node-2",
      app_name: "Firefox",
      executable: "firefox",
    });
    const replacementB = makeStream({
      id: "node-3",
      app_name: "Firefox",
      executable: "firefox",
    });
    const secondGraph = makeGraph([], [replacementA, replacementB]);
    const secondBuild = buildRoutingGraph(secondGraph);
    const positionA = secondBuild.nodes.find(
      (n) => n.id === streamNodeId("node-2"),
    )!.position;
    const positionB = secondBuild.nodes.find(
      (n) => n.id === streamNodeId("node-3"),
    )!.position;

    // Neither is placed via a stolen migration from the ambiguous old entry;
    // they should be auto-placed into distinct slots as normal.
    expect(positionA).not.toEqual(positionB);
  });
});

describe("connectivity-based 3-column layout (#342)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  function xFor(graph: ReturnType<typeof makeGraph>, nodeId: string): number {
    const built = buildRoutingGraph(graph);
    return built.nodes.find((n) => n.id === nodeId)!.position.x;
  }

  it("places a playback stream and a physical input device (mic) in the same left column", () => {
    const playback = makeStream({ id: "s1", direction: "playback" });
    const mic = makeDevice({ id: "mic", kind: "physical", direction: "input" });
    const graph = makeGraph([mic], [playback]);

    expect(xFor(graph, streamNodeId("s1"))).toBe(
      xFor(graph, deviceNodeId("mic")),
    );
  });

  it("places a capture stream, a physical output device, and a virtual output device in the same right column", () => {
    const capture = makeStream({ id: "s1", direction: "capture" });
    const speakers = makeDevice({
      id: "speakers",
      kind: "physical",
      direction: "output",
    });
    const virtualSink = makeDevice({
      id: "sink",
      kind: "virtual",
      direction: "output",
    });
    const graph = makeGraph([speakers, virtualSink], [capture]);

    const streamX = xFor(graph, streamNodeId("s1"));
    expect(xFor(graph, deviceNodeId("speakers"))).toBe(streamX);
    expect(xFor(graph, deviceNodeId("sink"))).toBe(streamX);
  });

  it("places a virtual input device (mic-mix bus) and a processing node in the same center column", () => {
    const virtualMic = makeDevice({
      id: "filtered-mic",
      kind: "virtual",
      direction: "input",
    });
    const proc = makeProcessingNode({ id: "proc-1" });
    const graph = makeGraph([virtualMic], [], [], [proc]);

    expect(xFor(graph, deviceNodeId("filtered-mic"))).toBe(
      xFor(graph, processingNodeNodeId("proc-1")),
    );
  });

  it("lays out a Discord/Slack-style mic chain strictly left to right (#342 repro)", () => {
    const mic = makeDevice({ id: "mic", kind: "physical", direction: "input" });
    const filteredMic = makeDevice({
      id: "filtered-mic",
      kind: "virtual",
      direction: "input",
      mix_sources: [{ device_id: "mic", volume_percent: 100, muted: false }],
    });
    const discordCapture = makeStream({
      id: "discord-capture",
      direction: "capture",
      current_target: "filtered-mic",
    });
    const graph = makeGraph([mic, filteredMic], [discordCapture]);

    const micX = xFor(graph, deviceNodeId("mic"));
    const filteredMicX = xFor(graph, deviceNodeId("filtered-mic"));
    const discordX = xFor(graph, streamNodeId("discord-capture"));

    expect(micX).toBeLessThan(filteredMicX);
    expect(filteredMicX).toBeLessThan(discordX);
  });
});

describe("row spacing scales with whether the graph has any processing node (#390)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  function yFor(graph: ReturnType<typeof makeGraph>, nodeId: string): number {
    const built = buildRoutingGraph(graph);
    return built.nodes.find((n) => n.id === nodeId)!.position.y;
  }

  it("uses the tight row height for an all-plain-card graph with no processing nodes", () => {
    const speakers = makeDevice({
      id: "speakers",
      kind: "physical",
      direction: "output",
    });
    const headphones = makeDevice({
      id: "headphones",
      kind: "physical",
      direction: "output",
    });
    const graph = makeGraph([speakers, headphones]);

    const gap = Math.abs(
      yFor(graph, deviceNodeId("speakers")) -
        yFor(graph, deviceNodeId("headphones")),
    );
    expect(gap).toBe(110);
  });

  it("uses the tall row height for the whole graph once any processing node is present", () => {
    const speakers = makeDevice({
      id: "speakers",
      kind: "physical",
      direction: "output",
    });
    const headphones = makeDevice({
      id: "headphones",
      kind: "physical",
      direction: "output",
    });
    const virtualMic = makeDevice({
      id: "filtered-mic",
      kind: "virtual",
      direction: "input",
    });
    const proc = makeProcessingNode({ id: "proc-1" });
    const graph = makeGraph([speakers, headphones, virtualMic], [], [], [proc]);

    const gap = Math.abs(
      yFor(graph, deviceNodeId("speakers")) -
        yFor(graph, deviceNodeId("headphones")),
    );
    expect(gap).toBe(280);
  });
});

describe("Bluetooth device icon parity (#226)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  function dataForDevice(overrides: Partial<Parameters<typeof makeDevice>[0]>) {
    const device = makeDevice(overrides);
    const graph = makeGraph([device], []);
    const node = buildRoutingGraph(graph).nodes.find(
      (n) => n.id === deviceNodeId(device.id),
    );
    return node?.data as RoutingGraphNodeData | undefined;
  }

  it("sets iconOverride to bluetooth for a bluez-named output device, without changing nodeClass", () => {
    const data = dataForDevice({
      system_name: "bluez_output.aa_bb_cc.1",
      direction: "output",
      kind: "physical",
    });
    expect(data?.nodeClass).toBe("output");
    expect(data?.iconOverride).toBe("bluetooth");
  });

  it("sets iconOverride to bluetooth for a bluez-named input device, without changing nodeClass", () => {
    const data = dataForDevice({
      system_name: "bluez_input.aa_bb_cc.1",
      direction: "input",
      kind: "physical",
    });
    expect(data?.nodeClass).toBe("input");
    expect(data?.iconOverride).toBe("bluetooth");
  });

  it("leaves iconOverride unset for a non-Bluetooth physical output", () => {
    const data = dataForDevice({
      system_name: "physical-out-1",
      direction: "output",
      kind: "physical",
    });
    expect(data?.nodeClass).toBe("output");
    expect(data?.iconOverride).toBeUndefined();
  });
});

describe("delay processing node (#313)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the Delay subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-delay-1",
      label: "Echo",
      kind: {
        kind: "delay",
        delay_ms: 350,
        feedback_percent: 40,
        feedforward_percent: -10,
      },
      system_name: "pipe-deck-proc-delay-echo",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find(
      (n) => n.id === processingNodeNodeId("proc-delay-1"),
    );
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("Delay");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({
      kind: "delay",
      delay_ms: 350,
      feedback_percent: 40,
      feedforward_percent: -10,
    });
    expect(data?.supportsEffects).toBe(false);
  });
});

describe("limiter processing node (#311)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the Limiter subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-limiter-1",
      label: "Safety Limiter",
      kind: {
        kind: "limiter",
        ceiling_db: -6,
        floor_db: -12,
        symmetric: false,
      },
      system_name: "pipe-deck-proc-limiter-safety-limiter",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find(
      (n) => n.id === processingNodeNodeId("proc-limiter-1"),
    );
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("Limiter");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({
      kind: "limiter",
      ceiling_db: -6,
      floor_db: -12,
      symmetric: false,
    });
    expect(data?.supportsEffects).toBe(false);
  });
});

describe("hpf processing node (#312)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the High-Pass Filter subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-hpf-1",
      label: "Rumble Filter",
      kind: { kind: "hpf", freq_hz: 150, resonance_x10: 12 },
      system_name: "pipe-deck-proc-hpf-rumble-filter",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find(
      (n) => n.id === processingNodeNodeId("proc-hpf-1"),
    );
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("High-Pass Filter");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({
      kind: "hpf",
      freq_hz: 150,
      resonance_x10: 12,
    });
    expect(data?.supportsEffects).toBe(false);
  });
});

describe("reverb processing node (#327)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the Reverb subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-reverb-1",
      label: "Room Verb",
      kind: { kind: "reverb", mix_percent: 35 },
      system_name: "pipe-deck-proc-reverb-room-verb",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find(
      (n) => n.id === processingNodeNodeId("proc-reverb-1"),
    );
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("Reverb");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({
      kind: "reverb",
      mix_percent: 35,
    });
    expect(data?.supportsEffects).toBe(false);
  });
});

describe("widener processing node (#314)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the Stereo Widener subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-widener-1",
      label: "Wide Stereo",
      kind: { kind: "widener", width_percent: 150 },
      system_name: "pipe-deck-proc-widener-wide-stereo",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find(
      (n) => n.id === processingNodeNodeId("proc-widener-1"),
    );
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("Stereo Widener");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({
      kind: "widener",
      width_percent: 150,
    });
    expect(data?.supportsEffects).toBe(false);
  });
});

describe("pan processing node (#16)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the Balance/Pan subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-pan-1",
      label: "Mic Balance",
      kind: { kind: "pan", balance_percent: 40 },
      system_name: "pipe-deck-proc-pan-mic-balance",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find(
      (n) => n.id === processingNodeNodeId("proc-pan-1"),
    );
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("Balance/Pan");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({
      kind: "pan",
      balance_percent: 40,
    });
    expect(data?.supportsEffects).toBe(false);
  });
});

describe("group processing node (issue #80)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the Group subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-group-1",
      label: "Speakers + Recorder",
      kind: { kind: "group", volume_percent: 100, muted: false },
      system_name: "pipe-deck-proc-group-speakers-recorder",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find(
      (n) => n.id === processingNodeNodeId("proc-group-1"),
    );
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("Group");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({
      kind: "group",
      volume_percent: 100,
      muted: false,
    });
    expect(data?.supportsEffects).toBe(false);
  });

  it("resolves each occupied output port to its member's id/label/portIndex", () => {
    const speakers = makeDevice({
      id: "dev-out-1",
      label: "Speakers",
      direction: "output",
    });
    const echo = makeDevice({
      id: "dev-out-2",
      label: "Echo Dot",
      direction: "output",
    });
    const node = makeProcessingNode({
      id: "proc-group-1",
      label: "Test Group",
      kind: { kind: "group", volume_percent: 100, muted: false },
      system_name: "pipe-deck-proc-group-test-group",
      outputs: [
        { index: 0, connected_id: "dev-out-1" },
        { index: 1, connected_id: "dev-out-2" },
      ],
    });
    const graph = makeGraph([speakers, echo], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find(
      (n) => n.id === processingNodeNodeId("proc-group-1"),
    );
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.groupMembers).toEqual([
      { id: "dev-out-1", label: "Speakers", portIndex: 0 },
      { id: "dev-out-2", label: "Echo Dot", portIndex: 1 },
    ]);
  });

  it("leaves groupMembers undefined for a non-group processing node", () => {
    const node = makeProcessingNode({
      id: "proc-fan-1",
      kind: { kind: "fan_out", volume_percent: 100, muted: false },
      outputs: [{ index: 0, connected_id: "dev-out-1" }],
    });
    const graph = makeGraph([makeDevice({ id: "dev-out-1" })], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find(
      (n) => n.id === processingNodeNodeId("proc-fan-1"),
    );
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.groupMembers).toBeUndefined();
  });
});
