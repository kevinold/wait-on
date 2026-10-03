@api
Feature: WAIT_ON_ENGINE picks the engine
  Unset or js runs the JS engine, rust runs the native addon and falls back to JS when it
  cannot load, rust-strict fails when it cannot load. Any other value is an error. Option
  errors are reported before engine errors.

  @kind:bad @route:none
  Scenario: an unknown engine is rejected
    Given an existing file
    And the environment variable WAIT_ON_ENGINE is "bogus"
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000 }
      """
    Then the wait rejects with an Error:
      """
      WAIT_ON_ENGINE="bogus" is not one of js, rust, rust-strict
      """

  @kind:bad @route:none
  Scenario: rust-strict fails when the host addon is missing
    Given the installed package has no addon for this host
    And an existing file
    And the environment variable WAIT_ON_ENGINE is "rust-strict"
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000 }
      """
    Then the wait rejects with an Error starting with:
      """
      WAIT_ON_ENGINE=rust-strict: failed to load the native addon at <addon>: Cannot find module
      """

  @kind:good @route:js
  Scenario: rust falls back to the JS engine when the host addon is missing
    Given the installed package has no addon for this host
    And an existing file
    And the environment variable WAIT_ON_ENGINE is "rust"
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000 }
      """
    Then the wait resolves

  @kind:bad @route:none
  Scenario: an option error is reported before an engine error
    Given the installed package has no addon for this host
    And the environment variable WAIT_ON_ENGINE is "rust-strict"
    When the consumer calls waitOn with:
      """
      { "resources": ["file:/wait-on-contract-never"], "timeout": 2000, "httpsAgent": {} }
      """
    Then the wait rejects with a ValidationError:
      """
      "httpsAgent" is not allowed
      """
