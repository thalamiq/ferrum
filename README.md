<div align="center">

# Ferrum

**A fast FHIR server built in Rust.**

[Documentation](https://docs.ferrum.thalamiq.io) | [Live Demo](https://ferrum.thalamiq.io) | [Test API](https://api.ferrum.thalamiq.io/fhir/metadata)

> This project is under active development. APIs may change at any time. If you encounter issues or spec compliance gaps, please [open an issue](https://github.com/thalamiq/ferrum/issues).

</div>

## Quickstart

```bash
curl -fsSL https://get.ferrum.thalamiq.io | sh
```

This starts the FHIR server, database, and admin UI. Access the API at `localhost:8080/fhir` and the admin UI at `localhost:3000`.

## Features

|                          |                                                                               | Progress |
| ------------------------ | ----------------------------------------------------------------------------- | -------- |
| **FHIR REST API**        | CRUD, conditional operations, search, batch/transaction bundles               | ✅       |
| **Search**               | Chaining, `_include`/`_revinclude`, full-text search, compartments            | ✅       |
| **Terminology Services** | `$expand`, `$lookup`, `$validate-code`, `$subsumes`, `$translate`, `$closure` | ✅       |
| **FHIRPath Engine**      | Full expression evaluator for querying and transforming resources             | ✅       |
| **Validation**           | Resource validation against profiles and constraints                          | 🟡       |
| **Snapshot Generation**  | StructureDefinition snapshots from differentials                              | 🟡       |
| **SMART on FHIR**        | OIDC based authentication and authorization                                   | 🟡       |
| **Admin UI**             | Web dashboard for resource browsing, monitoring, and administration           | ✅       |

## Architecture

```
┌──────────────┐    ┌──────────────┐    ┌───────────────┐
│   Admin UI   │    │  FHIR Server │    │    Worker     │
│   (Next.js)  │───▶│    (Axum)    │◀──▶│  (Background) │
└──────────────┘    └──────┬───────┘    └──────┬────────┘
                           │                   │
                           ▼                   ▼
                    ┌──────────────────────────────┐
                    │          PostgreSQL          │
                    └──────────────────────────────┘
```

## Documentation

Full documentation at [docs.ferrum.thalamiq.io](https://docs.ferrum.thalamiq.io).

## License

Licensed under the Apache License, Version 2.0. Copyright © 2026 Thalamiq GmbH.
