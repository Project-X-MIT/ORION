# API contract registry

All version 1 JSON responses use `ApiSuccess<T>` or `ApiFailure`. They include
`api_version` and the request identifier returned in the `X-Request-Id` header.
Clients branch on stable error codes, not human-readable messages.

| Operation ID | Owner | Method and path | Authentication |
| --- | --- | --- | --- |
| `health.get` | divi912 | `GET /health` | Public |
| `auth.register` | divi912 | `POST /api/v1/auth/register` | Public |
| `auth.login` | divi912 | `POST /api/v1/auth/login` | Public |
| `auth.logout` | divi912 | `POST /api/v1/auth/logout` | Authenticated |
| `auth.me` | divi912 | `GET /api/v1/auth/me` | Authenticated |
| `notifications.list` | divi912 | `GET /api/v1/notifications` | Authenticated |
| `notifications.mark_read` | divi912 | `PATCH /api/v1/notifications/{notification_id}` | Authenticated |
| `quiz.basic.get` | akaidk | `GET /api/v1/quiz/basic` | Authenticated |
| `quiz.basic.submit` | akaidk | `POST /api/v1/quiz/basic/attempts` | Authenticated |
| `quiz.advanced.get` | akaidk | `GET /api/v1/quiz/advanced` | Authenticated |
| `quiz.advanced.submit` | akaidk | `POST /api/v1/quiz/advanced/attempts` | Authenticated |
| `quiz.attempt.get` | akaidk | `GET /api/v1/quiz/attempts/{attempt_id}` | Authenticated |
| `leaderboard.list` | ShauryaBijalwan | `GET /api/v1/leaderboard` | Public |
| `profile.get` | ShauryaBijalwan | `GET /api/v1/profiles/{user_id}` | Public |
| `research.create` | shivanshrawat13aug2007-commits | `POST /api/v1/research` | Authenticated |
| `research.update` | shivanshrawat13aug2007-commits | `PUT /api/v1/research/{research_id}` | Authenticated |
| `research.submit` | shivanshrawat13aug2007-commits | `POST /api/v1/research/{research_id}/submission` | Authenticated |
| `research.review` | shivanshrawat13aug2007-commits | `POST /api/v1/research/{research_id}/reviews` | Reviewer |
| `research.reviews.get` | shivanshrawat13aug2007-commits | `GET /api/v1/research/{research_id}/reviews` | Author |
| `research.list_published` | shivanshrawat13aug2007-commits | `GET /api/v1/research` | Public |
| `research.get` | shivanshrawat13aug2007-commits | `GET /api/v1/research/{research_id}` | Public |
| `news.list` | sudhanshu001122 | `GET /api/v1/news` | Public |
| `learning.course.get` | sudhanshu001122 | `GET /api/v1/learning/courses/{course_id}` | Public |
| `learning.progress.get` | sudhanshu001122 | `GET /api/v1/learning/progress` | Authenticated |
| `learning.lesson.complete` | sudhanshu001122 | `POST /api/v1/learning/lessons/{lesson_id}/completion` | Authenticated |
| `discord.connect` | sudhanshu001122 | `GET /api/v1/discord/connect` | Public |

Adding a response field is compatible when clients may safely ignore it.
Removing or renaming a field, changing its type, or changing route semantics
requires a new API version and an ADR.
