import { ApiError } from "./api"

export function loginErrorMessage(error: unknown): string {
  return error instanceof ApiError ? error.message : "登录失败，请稍后重试"
}
