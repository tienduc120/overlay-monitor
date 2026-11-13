---
name: new-service
description: Use when adding a service or bounded context under services/. Generates it from the witness so it starts green, then replaces the sample aggregate with the real domain.
---

# Add a service

A service is one bounded context. The generator copies the witness `services/example`, which passes every gate, so the new service starts green and the first thing you change is the domain.

## Steps

1. **Name it.** The service is lowercase kebab-case (`billing`, `order-intake`); its first aggregate is PascalCase (`Invoice`). If you cannot name the aggregate, you do not yet know what the service owns: ask the user. Done when both names are settled.
2. **Generate.** `cargo xtask new-service <name> --aggregate <Name>`. Read the file list it prints. Plurals are the singular plus "s"; fix an irregular one by hand.
3. **Prove it green.** `cargo nextest run -p <name>-domain -p <name>-application -p <name>-infrastructure -p <name>-interfaces -p <name>-api`, then commit the untouched copy (`feat(<name>): add the service`) so the next diff shows only your domain.
4. **Change the domain test first, then the domain.** Replace the sample fields and rules with the real invariants. `new` stays the only way to build a valid value; time stays an argument.
5. **Grow outward**, one red test at a time: a new behaviour is a use case in `application` (skill: `writing-use-cases`), a new dependency is a port there with an adapter in `infrastructure`, a new endpoint is a route in `interfaces`. The layer rules name any edge that crosses the wrong way.
6. **Done** when the sample aggregate's name is gone from the service and its crates' tests are green.

## The witness

`services/example` is the template every future service is copied from. Change it only to change every future service, keep it green, and keep its words (`example`, `Note`, `note`) out of its prose so the rename cannot hit them.
