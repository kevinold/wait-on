// commitlint config — CI lints PR commits against Conventional Commits with it. The local gate is
// the dependency-free .githooks/commit-msg (enable with `cargo xtask hooks`); see AGENTS.md
// "Commit messages".
//
// KTD6: three in-flight contributor commits carry non-conventional subjects. Ignore those exact
// subjects so PRs #228/#233/#235 do not turn red while they are open. Keep the contributors'
// authorship. REMOVE this `ignores` block (and this comment) once those PRs merge upstream.
const inFlightSubjects = [
  "improve: use Node's util.parseArgs over `minimist`", // d76fa26
  '[Api]: QOL imporvement. Allow string as opts.', // dae5e74
  'Support Windows named pipe paths in http://unix: resources and run tests on Windows', // b594acb
];

// #82: five commits already on the protected spike-next-rs fail the rules, so spike PR #51
// (next..spike-next-rs) cannot go green by rewriting them. Match each exact full message, never a
// header alone, so a new commit reusing one of these headers is still linted. REMOVE this block
// (and this comment) once spike-next-rs merges into next.
const pushedSpikeMessages = [
  // cccba80
  'fix(xtask): never rebuild the running xtask binary on Windows (#75)\n\n' +
    'Inner cargo calls from cargo xtask (ci, fmt, lint, test, cov) now build into ' +
    '<target>/xtask-inner, so cargo test --workspace never tries to relink the running ' +
    "target/debug/xtask.exe, which Windows locks (os error 5). xtask's own tests and clippy stay " +
    'in the gate.',
  // 1649323
  'fix(review): apply review findings\n\n' +
    'Run cargo vet before cargo builds xtask (ci:rs alias), restore the 30s per-spawn kill in ' +
    'bench-startup, share one exit-code wrapper and a lossy env snapshot, and refresh the vet ' +
    'baseline counts in the guides.',
  'docs(plans): L12 xtask + Justfile lane plan', // 9347b8b
  'feat(rust): verified TLS roots, client identity, explicit proxy and unix/pipe transport in the http checker (#57)', // b171574
  'docs(plans): L6 command implementation plan (#58)', // 3688229
];

module.exports = {
  extends: ['@commitlint/config-conventional'],
  ignores: [
    (message) => inFlightSubjects.includes(message.split('\n')[0].trim()),
    (message) => pushedSpikeMessages.includes(message.trim()),
  ],
};
