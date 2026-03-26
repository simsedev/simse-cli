---
name: code-review
description: Perform a structured code review on the current changes or a specific file
---

# Code Review

Review the code specified by $ARGUMENTS using this structured approach:

## 1. Correctness
- Does the code do what it claims to do?
- Are there logic errors or off-by-one mistakes?
- Are edge cases handled?

## 2. Security
- Input validation on all boundaries?
- No SQL injection, XSS, command injection?
- Secrets not hardcoded?

## 3. Performance
- No unnecessary allocations in hot paths?
- Database queries efficient (no N+1)?
- Appropriate caching?

## 4. Readability
- Clear naming?
- Comments where logic is non-obvious?
- Consistent style with the codebase?

## 5. Testing
- Are the changes tested?
- Do existing tests still pass?
- Are edge cases covered?

Provide a summary with: **Approve**, **Request Changes**, or **Needs Discussion**.
