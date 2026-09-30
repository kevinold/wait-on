'use strict';

// Stands in for the native addon so the addon-present branch runs without a build.
module.exports = {
  version: () => 'fake',
  noop() {}
};
