# Flow API Reference (MVP)

Base URL: `http://localhost:8080/api/v1`

## Authentication

### Start Registration
```
GET /webauthn/start_registration
```
Returns WebAuthn challenge for passkey registration.

### Finish Registration
```
POST /webauthn/finish_registration
Body: { "challenge_id": string, "credential": PublicKeyCredential }
```

### Start Authentication
```
POST /webauthn/start_authentication
```

### Finish Authentication
```
POST /webauthn/finish_authentication
Body: { "challenge_id": string, "credential": PublicKeyCredential }
Returns: { "verified": true, "token": string, "did": string }
```

## Spaces

### List Spaces
```
GET /spaces
Returns: [{ "id": number, "key": string, "location": string, "time_created": string }]
```

### Create Space
```
POST /spaces
Body: { "dir": string }
```

### Get Space
```
GET /spaces/:key
Returns: { "id": number, "key": string, "location": string, "time_created": string }
```

### Delete Space
```
DELETE /spaces/:key
```

### Get Indexing Status
```
GET /spaces/:key/status
Returns: {
  "indexing_in_progress": boolean,
  "last_indexed": string | null,
  "files_indexed": number,
  "chunks_stored": number,
  "files_failed": number,
  "last_error": string | null
}
```

### Reindex Space
```
POST /spaces/:key/reindex
```

## Query

### Search Space
```
GET /spaces/search?space_key=KEY&query=QUERY
Returns: { "success": true, "response": string }
```

## Knowledge Graph

### List Entities
```
GET /spaces/:key/entities
Returns: [{
  "id": number,
  "cid": string,
  "name": string,
  "entity_type": string,
  "properties": object,
  "created_at": string
}]
```

### Get Entity with Edges
```
GET /spaces/:key/entities/:id
Returns: {
  "id": number,
  "cid": string,
  "name": string,
  "entity_type": string,
  "properties": object,
  "created_at": string,
  "edges": [{
    "id": number,
    "edge_type": string,
    "source_id": number | null,
    "target_id": number | null,
    "direction": "incoming" | "outgoing"
  }]
}
```

## Network

### Get Status
```
GET /network/status
```

### Get Peers
```
GET /network/peers
```

### Start Network
```
POST /network/start
```

### Stop Network
```
POST /network/stop
```

### Dial Peer
```
POST /network/dial
Body: { "address": string }
```

## Health

### Health Check
```
GET /health
Returns: { "status": "healthy", "timestamp": string }
```

## Error Responses

All errors follow this format:
```json
{
  "success": false,
  "error": {
    "code": "ERROR_CODE",
    "message": "Human readable message"
  }
}
```

Error codes:
- `AUTH_FAILED` (401)
- `NOT_FOUND` (404)
- `VALIDATION_ERROR` (400)
- `SPACE_EXISTS` (409)
- `INDEXING_IN_PROGRESS` (409)
- `LLM_ERROR` (503)
- `INTERNAL_ERROR` (500)
