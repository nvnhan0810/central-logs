/** The signed-in person, as known to the backend. Carries no behaviour beyond display rules. */
export interface User {
  /** Stable identifier issued by the identity provider. */
  readonly subject: string;
  readonly email: string | null;
  readonly name: string | null;
}

/** Name to show in the UI: the display name, else the email, else the subject. */
export function displayName(user: User): string {
  const candidates = [user.name, user.email];
  for (const candidate of candidates) {
    if (candidate !== null && candidate.trim() !== '') {
      return candidate.trim();
    }
  }
  return user.subject;
}
