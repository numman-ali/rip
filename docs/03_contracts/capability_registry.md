# Capability Registry (Source of Truth)

Summary
- This is the single source of truth for capability ids and versions.
- Each capability must link to tests and surface adapters.

Format (per entry)
- id
- version
- intent
- inputs
- outputs
- errors
- surfaces

Registry
- session.create (v1)
- session.send_input (v1)
- session.stream_events (v1)
- session.cancel (v1)
