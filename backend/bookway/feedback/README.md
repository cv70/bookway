# Feedback

`feedback` owns product feedback submitted by authenticated users. It stores
the original report, its contact preference and client context, lets a user
read their own history, and exposes a service-token-protected queue for
moderators to process or close reports.

The public path is handled only by Gateway:

- `POST /v1/feedback`
- `GET /v1/me/feedback`

Moderation endpoints are intentionally Gateway role-protected:

- `GET /v1/moderation/feedback`
- `PATCH /v1/moderation/feedback/{feedback_id}`
