/** Process helpers for the deploy CLI. Thin wrappers over Bun.spawn. */

export interface RunOpts {
  cwd?: string;
  env?: Record<string, string | undefined>;
  /** Written to stdin then closed (for `docker login --password-stdin`, psql heredocs). */
  input?: string;
}

function mergedEnv(extra?: Record<string, string | undefined>): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(process.env)) if (v !== undefined) out[k] = v;
  if (extra) for (const [k, v] of Object.entries(extra)) if (v !== undefined) out[k] = v;
  return out;
}

/** Run a command with inherited stdio. Throws on a non-zero exit. */
export async function run(cmd: string[], opts: RunOpts = {}): Promise<void> {
  const proc = Bun.spawn(cmd, {
    cwd: opts.cwd,
    env: mergedEnv(opts.env),
    stdin: opts.input !== undefined ? "pipe" : "inherit",
    stdout: "inherit",
    stderr: "inherit",
  });
  if (opts.input !== undefined) {
    proc.stdin!.write(opts.input);
    await proc.stdin!.end();
  }
  const code = await proc.exited;
  if (code !== 0) throw new Error(`command failed (exit ${code}): ${cmd.join(" ")}`);
}

export interface CaptureResult {
  code: number;
  stdout: string;
  stderr: string;
}

/** Run a command capturing stdout/stderr. Does not throw on non-zero exit. */
export async function capture(cmd: string[], opts: RunOpts = {}): Promise<CaptureResult> {
  const proc = Bun.spawn(cmd, {
    cwd: opts.cwd,
    env: mergedEnv(opts.env),
    stdin: opts.input !== undefined ? "pipe" : "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  if (opts.input !== undefined) {
    proc.stdin!.write(opts.input);
    await proc.stdin!.end();
  }
  const [stdout, stderr] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
  ]);
  const code = await proc.exited;
  return { code, stdout, stderr };
}
