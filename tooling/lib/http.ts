/** Poll an HTTP endpoint until it answers with a 2xx (or time out). */
export async function waitForHttp(url: string, timeoutMs = 60_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {
      // connection refused while the service is still starting; keep polling
    }
    if (Date.now() > deadline) {
      throw new Error(`${url} not ready after ${timeoutMs}ms`);
    }
    await Bun.sleep(1000);
  }
}
