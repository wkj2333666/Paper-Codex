import { expect, test } from "vitest"
import { ApiError } from "./api"
import { loginErrorMessage } from "./login"

test("login displays the server throttling message", () => {
  expect(loginErrorMessage(new ApiError(429, "登录尝试过于频繁，请稍后重试"))).toBe(
    "登录尝试过于频繁，请稍后重试",
  )
})

test("login falls back to a safe message for network errors", () => {
  expect(loginErrorMessage(new TypeError("network unavailable"))).toBe("登录失败，请稍后重试")
})
