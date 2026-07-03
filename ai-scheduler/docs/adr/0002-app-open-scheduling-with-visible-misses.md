# App-open scheduling with visible misses

AI Scheduler will run routines only while the desktop app is open. If a scheduled time passes while the app is closed, the app records a missed run for that occurrence and schedules the next occurrence instead of running late. This avoids hidden background work and prevents surprising catch-up execution when the user reopens the app.
