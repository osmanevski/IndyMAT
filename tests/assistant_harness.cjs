// check.py starts a server before each test. These tests need executable overrides
// in the server's environment, so run a second, isolated source copy. Never change
// or stop the user's running app and never inject a test-only HTTP configuration.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { spawn } = require('node:child_process');
const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));
async function assistantServer() {
  const root = path.resolve(__dirname, '..');
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'indymat-assistant-'));
  for (const name of ['app.py', 'backend', 'octave', 'static']) fs.cpSync(path.join(root, name), path.join(directory, name), { recursive: true });
  const workspace = path.join(directory, 'workspace');
  fs.mkdirSync(workspace);
  fs.writeFileSync(path.join(workspace, 'sample.m'), 'initial = 1;\n');
  const fixture = path.join(root, 'tests', 'fixtures', 'assistant_cli.py');
  const proc = spawn('python3', ['app.py', '--no-browser', '--port', '0', '--workspace', workspace], {
    cwd: directory,
    env: { ...process.env, INDYMAT_ASSISTANT_CLAUDE: fixture, INDYMAT_ASSISTANT_CODEX: fixture, INDYMAT_ASSISTANT_AGY: fixture },
    stdio: ['ignore', 'pipe', 'pipe']
  });
  // Startup output is kept private: launch credentials are never printed.
  let output = '';
  proc.stdout.on('data', (data) => { output = (output + data).slice(-4000); });
  proc.stderr.on('data', (data) => { output = (output + data).slice(-4000); });
  let launch;
  let base;
  let token;
  const request = (endpoint, body, headers = {}) => fetch(base + 'api/' + endpoint, {
    method: body === undefined ? 'GET' : 'POST',
    headers: { 'X-MF-Token': token, 'X-MF-Language': 'tr', ...(body === undefined ? {} : { 'Content-Type': 'application/json' }), ...headers },
    body: body === undefined ? undefined : JSON.stringify(body)
  });
  const close = async () => {
    try { if (base) await request('shutdown', {}); } catch {}
    for (let index = 0; index < 80 && proc.exitCode === null; index++) await delay(50);
    if (proc.exitCode === null) proc.kill('SIGTERM');
    await new Promise((resolve) => {
      if (proc.exitCode !== null) return resolve();
      proc.once('exit', resolve);
      setTimeout(() => { proc.kill('SIGKILL'); resolve(); }, 2000).unref();
    });
    fs.rmSync(directory, { recursive: true, force: true });
  };
  try {
    for (let index = 0; index < 200; index++) {
      if (proc.exitCode !== null) throw new Error('Assistant test server exited before startup');
      try { launch = JSON.parse(fs.readFileSync(path.join(directory, '.matlab-free', 'launch.json'), 'utf8')); break; } catch {}
      await delay(50);
    }
    if (!launch) throw new Error('Assistant test server startup timeout');
    [base, token] = launch.url.split('#');
    for (let index = 0; index < 300; index++) {
      const state = await (await request('state')).json();
      if (state.status === 'idle') return { request, close, launch, base, token, workspace };
      if (state.status === 'dead') throw new Error('Assistant test server engine did not start');
      await delay(50);
    }
    throw new Error('Assistant test server engine startup timeout');
  } catch (error) {
    await close();
    throw error;
  }
}
module.exports = { assistantServer, delay };
