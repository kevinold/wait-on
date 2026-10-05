'use strict';

// node run.js '<json opts>' [--callback]
// Calls the installed wait-on and prints one JSON line after any log output:
// {exportType, outcome, errorName, errorMessage, cbCalls, cbArg, cbSync, returned, elapsedMs}.
// JSON has no functions, so a string validateStatus is the body of `function (status)`.

const waitOn = require('wait-on');

const opts = JSON.parse(process.argv[2]);
if (typeof opts.validateStatus === 'string') opts.validateStatus = new Function('status', opts.validateStatus);
const start = Date.now();
const result = { exportType: typeof waitOn, cbCalls: 0 };
const settle = (err) => {
  result.outcome = err ? 'rejected' : 'resolved';
  if (err) {
    result.errorName = err.name;
    result.errorMessage = err.message;
  }
  result.elapsedMs = Date.now() - start;
  // a handle left open after settling (9.5.1's unanswered axios request) would keep the
  // process from ever reaching beforeExit: report and exit anyway
  setTimeout(() => {
    report();
    process.exit();
  }, 500).unref();
};
let reported = false;
const report = () => {
  if (!reported) console.log(JSON.stringify(result));
  reported = true;
};

if (process.argv.includes('--callback')) {
  let sync = true;
  const returned = waitOn(opts, (err) => {
    result.cbCalls += 1;
    if (result.cbCalls === 1) {
      result.cbSync = sync;
      result.cbArg = err === undefined ? 'undefined' : err instanceof Error ? 'Error' : String(err);
      settle(err);
    }
    return 'callback-return';
  });
  sync = false;
  result.returned = returned === undefined ? 'undefined' : String(returned);
} else {
  const returned = waitOn(opts);
  result.returned = returned instanceof Promise ? 'Promise' : String(returned);
  returned.then(() => settle(), settle);
}

process.once('beforeExit', report);
