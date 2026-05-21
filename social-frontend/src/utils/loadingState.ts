/**
 * Bucket-based loading state for the header progress bar.
 *
 * Every `fetchApi` call reserves one unit on `startLoad` and closes it on
 * `endLoad`. The bar is visible while any unit is open; width is the ratio
 * of closed to total units.
 *
 * Sequential loop callers pass `hasProgress: [batchNumber, totalBatches]`
 * so the loop counts as one logical unit even though it makes N fetchApi
 * calls. The first batch (`current === 1`) reserves the unit; each batch's
 * `endLoad` closes `1 / totalBatches` of that unit.
 *
 * When the bucket drains (closed >= total) it resets so the next flurry
 * starts at 0%. `resetLoadCount` is exposed for explicit resets, e.g. on
 * route change.
 */

export interface LoadingState {
  /** 0-100. Width of the bar. */
  progress: number;
  /** Whether the bar should be on screen (drives opacity). */
  visible: boolean;
}

export interface LoadOptions {
  /** `[batchNumber, totalBatches]` for serial loop callers. */
  hasProgress?: [number, number];
}

type Listener = () => void;

/// Floor applied while the bar is visible, so a request whose ratio is
/// near 0 still shows something.
const MIN_VISIBLE_PROGRESS = 8;

/// Fractional accounting (loop batches close 1/N per call) accumulates
/// floating-point error: N calls of 1/N sum to e.g. 2.9999999999999996,
/// not 3. EPSILON absorbs that drift so `closed >= total` checks don't
/// hang the bar a hair short of completion.
const EPSILON = 1e-6;

let total = 0;
let closed = 0;
let snapshot: LoadingState = { progress: 0, visible: false };
const listeners = new Set<Listener>();

function compute(): LoadingState {
  const visible = total > 0 && closed < total - EPSILON;
  let progress = total > 0 ? (closed / total) * 100 : 0;
  if (visible && progress < MIN_VISIBLE_PROGRESS) progress = MIN_VISIBLE_PROGRESS;
  if (progress > 100) progress = 100;
  return { progress, visible };
}

function emit(): void {
  snapshot = compute();
  for (const l of listeners) l();
  // Auto-drain when the bucket is balanced (or stale completions have
  // pushed `closed` past `total`). The `closed > 0` guard keeps an idle
  // bucket from re-resetting on every emit.
  if (closed > 0 && closed >= total - EPSILON) {
    total = 0;
    closed = 0;
  }
}

export function startLoad(opts?: LoadOptions): void {
  if (opts?.hasProgress) {
    const [current] = opts.hasProgress;
    // Only the first batch in a serial loop reserves a unit; subsequent
    // batches are continuations of the same logical operation.
    if (current === 1) total += 1;
  } else {
    total += 1;
  }
  emit();
}

export function endLoad(opts?: LoadOptions): void {
  if (opts?.hasProgress) {
    const [, target] = opts.hasProgress;
    if (target > 0) closed += 1 / target;
  } else {
    closed += 1;
  }
  emit();
}

export function resetLoadCount(): void {
  total = 0;
  closed = 0;
  snapshot = compute();
  for (const l of listeners) l();
}

export function subscribeLoading(l: Listener): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function getLoadingSnapshot(): LoadingState {
  return snapshot;
}
