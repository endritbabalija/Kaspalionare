#!/usr/bin/env node
// PreToolUse hook: blocks Bash commands that could touch Kaspa mainnet.
// Exit code 2 blocks the command; the message on stderr is shown to Claude.

let raw = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => { raw += chunk; });
process.stdin.on('end', () => {
  let cmd = '';
  try {
    cmd = String(JSON.parse(raw)?.tool_input?.command ?? '');
  } catch {
    process.exit(0); // unreadable input: no decision
  }

  const reasons = [];

  // Mainnet addresses start with "kaspa:" (testnet uses "kaspatest:", devnet/simnet differ).
  if (/\bkaspa:[qp][0-9a-z]{40,}/i.test(cmd)) reasons.push('a mainnet address (kaspa:...)');

  // Default rusty-kaspa mainnet RPC ports.
  if (/:(16110|17110|18110)\b/.test(cmd)) reasons.push('a mainnet RPC port (16110/17110/18110)');

  // Explicitly selecting mainnet.
  if (/--network[ =]+mainnet\b/i.test(cmd) || /--mainnet\b/i.test(cmd)) reasons.push('an explicit mainnet flag');

  // kaspad defaults to mainnet unless told otherwise.
  if (/\bkaspad(\.exe)?\b/i.test(cmd) && !/--(testnet|devnet|simnet)\b/i.test(cmd)) {
    reasons.push('kaspad without --testnet/--devnet/--simnet (it defaults to mainnet)');
  }

  if (reasons.length) {
    process.stderr.write(
      'Blocked by project hook: this command uses ' + reasons.join(', ') + '. ' +
      'This project runs on local tests and testnet-10 only. See CLAUDE.md > Safety.\n'
    );
    process.exit(2);
  }
  process.exit(0);
});
