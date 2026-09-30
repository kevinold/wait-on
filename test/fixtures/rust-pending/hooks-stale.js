'use strict';

const { createHooks } = require('../../rust-pending');

exports.mochaHooks = createHooks(['no such test']);
