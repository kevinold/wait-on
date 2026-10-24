@api
Feature: HTTP request options
  headers and auth are sent with every check, with fetch's default accept-encoding unless
  the caller sets one; redirects are followed (21 of them, keeping an empty query) unless
  followRedirect is false, validateStatus replaces the 2xx rule, and httpTimeout fails a
  check whose response does not arrive in time. A port on the Fetch bad-port list (6000,
  6665-6669, 10080, ...) is reached like any other.

  @kind:good
  Scenario: a server on a Fetch bad-list port is reached
    Given an HTTP server on a Fetch bad-list port answering 200
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000 }
      """
    Then the wait resolves

  @kind:good
  Scenario: headers reach the server
    Given an HTTP server answering 200 that records requests
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000, "headers": { "x-contract": "yes" } }
      """
    Then the wait resolves
    And the server saw the header "x-contract" as "yes"

  @kind:good
  Scenario: the request keeps an empty query and sends the default accept-language
    Given an HTTP server answering 200 that records requests to "/health?"
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000 }
      """
    Then the wait resolves
    And the server saw the request path "/health?"
    And the server saw the header "accept-language" as "*"

  @kind:good
  Scenario: the default accept-encoding is sent, so a server that requires one answers
    Given an HTTP server answering 406 to a request without accept-encoding
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000 }
      """
    Then the wait resolves
    And the server saw the header "accept-encoding" as "gzip, deflate"

  @kind:good
  Scenario: a caller accept-encoding replaces the default
    Given an HTTP server answering 406 to a request without accept-encoding
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000, "headers": { "Accept-Encoding": "br" } }
      """
    Then the wait resolves
    And the server saw the header "accept-encoding" as "br"

  @kind:good
  Scenario: auth is sent as a Basic authorization header
    Given an HTTP server answering 200 that records requests
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000, "auth": { "username": "user", "password": "pass" } }
      """
    Then the wait resolves
    And the server saw the header "authorization" as "Basic dXNlcjpwYXNz"

  @kind:good
  Scenario: a redirect is followed by default
    Given an HTTP server redirecting to a page answering 200
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000 }
      """
    Then the wait resolves

  @kind:good
  Scenario: a redirect to a path with an empty query keeps the query
    Given an HTTP server redirecting to "/final?"
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000 }
      """
    Then the wait resolves
    And the redirect was followed to the path "/final?"

  @kind:good
  Scenario: a 2xx after 21 redirects is reached
    Given an HTTP server redirecting 21 times before answering 200
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000 }
      """
    Then the wait resolves

  @kind:bad
  Scenario: a 22nd redirect fails the check
    Given an HTTP server redirecting 22 times before answering 200
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 600, "interval": 100 }
      """
    Then the wait rejects with an Error:
      """
      Timed out waiting for: <resource 1>
      """

  @kind:bad
  Scenario: followRedirect false treats the redirect as the answer
    Given an HTTP server redirecting to a page answering 200
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 600, "interval": 100, "followRedirect": false }
      """
    Then the wait rejects with an Error:
      """
      Timed out waiting for: <resource 1>
      """

  @kind:good
  Scenario: validateStatus accepts a status outside 2xx
    Given an HTTP server answering 403
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000, "validateStatus": "return status === 403" }
      """
    Then the wait resolves

  @kind:bad
  Scenario: without validateStatus a 403 is not available
    Given an HTTP server answering 403
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 600, "interval": 100 }
      """
    Then the wait rejects with an Error:
      """
      Timed out waiting for: <resource 1>
      """

  @kind:good
  Scenario: httpTimeout fails a check that gets no answer, which satisfies a reverse wait
    Given an HTTP server that never answers
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 2000, "interval": 100, "reverse": true, "httpTimeout": 200 }
      """
    Then the wait resolves

  @kind:bad
  Scenario: without httpTimeout a check that gets no answer keeps a reverse wait waiting
    Given an HTTP server that never answers
    When the consumer calls waitOn with:
      """
      { "resources": ["<resource 1>"], "timeout": 1000, "interval": 100, "reverse": true }
      """
    Then the wait rejects with an Error:
      """
      Timed out waiting for: <resource 1>
      """
