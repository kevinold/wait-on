'use strict';

// The consumer contract (features/). Support code is only features/support: cucumber would
// otherwise load the fixture runners under features/fixtures as step definitions.
// Run it through `npm run contract`, which installs the packed package first.
module.exports = {
  default: {
    paths: ['features/**/*.feature'],
    require: ['features/support/**/*.js']
  }
};
