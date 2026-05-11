# Testing Requirements

All implementation plans must include unit tests for every added feature.

When writing implementation plans:
- Every task that adds or modifies a formula must include a test step with specific test cases
- Pure functions (math, resolution logic) get unit tests
- System-level behavior that's hard to unit test in Bevy should get testable helper functions extracted, then those helpers get tested
- Status effect logic (stacking, refresh, tick-down) should have dedicated test functions
- Each task's commit should include its tests — don't batch tests into a separate task at the end
