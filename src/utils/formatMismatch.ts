export interface AudioFormatInfo {
  sample_rate?: number;
  channels?: number;
}

export interface FormatMismatchInfo {
  mismatch: boolean;
  title?: string;
}

/** Compares two endpoints' negotiated audio format. Only flags a mismatch
 * when both sides report a known, differing value — an unknown value on
 * either side isn't treated as a mismatch. PipeWire already resamples/remixes
 * transparently at the link layer, so this is informational, not an error. */
export function formatMismatch(
  a: AudioFormatInfo,
  b: AudioFormatInfo,
): FormatMismatchInfo {
  const rateMismatch =
    a.sample_rate != null &&
    b.sample_rate != null &&
    a.sample_rate !== b.sample_rate;
  const channelMismatch =
    a.channels != null && b.channels != null && a.channels !== b.channels;

  if (!rateMismatch && !channelMismatch) {
    return { mismatch: false };
  }

  const parts: string[] = [];
  if (rateMismatch) {
    parts.push(`${a.sample_rate} Hz → ${b.sample_rate} Hz`);
  }
  if (channelMismatch) {
    parts.push(`${a.channels}ch → ${b.channels}ch`);
  }

  return {
    mismatch: true,
    title: `Format differs from target (${parts.join(", ")}) — PipeWire resamples/remixes this automatically.`,
  };
}
