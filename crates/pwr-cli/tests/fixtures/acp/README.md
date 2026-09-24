# Agent Client Protocol schema

`schema-v1.json` is the published ACP v1 JSON schema, copied unchanged from
`agentclientprotocol/agent-client-protocol` at commit
`4effcc11e117c67feb5ed505b17f75537932f5a6` (`schema/v1/schema.json`, 2026-08-20),
under the Apache License 2.0.

`pwr serve`'s protocol tests validate every message the server sends against
it. Updating the schema is a protocol upgrade: replace the file, record the new
commit here, and read the diff for changed method shapes.
