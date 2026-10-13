"use strict";
// Completion latency, not a GPU timer. Setup is outside begin()/complete();
// two completed warm-up frames absorb deferred driver work. Only sustained
// slow completed frames cause the performance fallback. Blank time has its
// own wall-clock bound, enforced even during warm-up and cancelled renders.
const TIMING_LIMITS = Object.freeze({ warmupFrames: 2, slowFrames: 3, timeMs: 250, blankMs: 5000 });
class TransparencyTiming {
  constructor(clock = () => performance.now()) { this.clock = clock; this.reset(); }
  reset() { this.frames = 0; this.slowFrames = 0; }
  begin() { return this.clock(); }
  complete(started) { return this.record(Math.max(0, this.clock() - started)); }
  record(elapsed) {
    const warmup = this.frames++ < TIMING_LIMITS.warmupFrames;
    this.slowFrames = !warmup && elapsed > TIMING_LIMITS.timeMs ? this.slowFrames + 1 : 0;
    const diagnostics = { frameMs: elapsed, frames: this.frames, warmup, guarded: !warmup, slowFrames: this.slowFrames };
    return { diagnostics, failure: this.slowFrames >= TIMING_LIMITS.slowFrames ? {
      actual_ms: elapsed, limit_ms: TIMING_LIMITS.timeMs, phase: "steady_frames"
    } : null };
  }
}
module.exports = { TIMING_LIMITS, TransparencyTiming };
