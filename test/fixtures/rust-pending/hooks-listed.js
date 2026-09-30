'use strict';

const { createHooks } = require('../../rust-pending');

exports.mochaHooks = createHooks(['rust-pending fixture listed test']);
