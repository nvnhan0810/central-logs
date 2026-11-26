/** Text to show for an error thrown by a use case or the gateway. */
export function errorMessage(error: unknown): string {
  return error instanceof Error && error.message !== '' ? error.message : 'Có lỗi xảy ra';
}
