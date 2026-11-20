# General Application Tips

## Tree architecture

> [!TIP]
> **Use a separate crate for each layer.** Organize your workspace into a domain crate, an
> application crate, an infrastructure crate, a presentation crate and a main crate. Crate
> boundaries make dependencies explicit and let the compiler prevent layers from accessing
> code they should not depend on.

Think of this structure as a **tree architecture**:

- **Soil:** utility crates and dependencies, including `croisiere`, support the layers above them.
- **Roots:** the domain crate defines business values, identities and behavior.
- **Trunk:** the application crate depends on the domain and coordinates use cases.
- **Branches:** the infrastructure crate implements storage and external integrations, while the
  presentation crate exposes the application's use cases. These are separate branches; neither
  should depend on the other.

The main crate assembles the application. It selects infrastructure implementations, supplies them
to the application and starts the presentation entry point. Keep business logic in the domain and
use-case coordination in the application rather than putting them in `main`.

Dependencies follow the tree toward its roots: infrastructure and presentation depend on the
application, and the application depends on the domain. The main crate connects these crates.
Keep utility crates independent of the layers they support so they do not introduce a dependency
back toward the branches.

> [!TIP]
> **Consider giving each presentation its own crate.** An HTTP API, a command-line interface and
> a desktop interface can each be a separate branch using the same application crate. This keeps
> their dependencies and entry points independent while sharing use cases and business rules.
>
> Splitting infrastructure into several crates can also be useful. For example, database
> persistence, email delivery and an external service integration can each have their own crate.
> The main crate selects and wires the components it needs. Split where responsibilities and
> dependencies are distinct enough to benefit from their own boundary.
