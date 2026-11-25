import type { User } from '@/domain/auth/user';

/** Wire format of `GET /api/auth/me` (see backend/src/interfaces/http/auth.rs `UserResponse`). */
export interface UserDto {
  readonly subject: string;
  readonly email: string | null;
  readonly name: string | null;
}

export function isUserDto(value: unknown): value is UserDto {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  return (
    'subject' in value &&
    typeof value.subject === 'string' &&
    'email' in value &&
    (value.email === null || typeof value.email === 'string') &&
    'name' in value &&
    (value.name === null || typeof value.name === 'string')
  );
}

export function toUser(dto: UserDto): User {
  return { subject: dto.subject, email: dto.email, name: dto.name };
}
