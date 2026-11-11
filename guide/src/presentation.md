# Cruise: Presentation

The presentation is how the outside world talks to the application: a web API, a command-line
interface, a graphical interface. Whatever its form, its job is the same: turn what it receives into
[commands and queries](./application/commands_and_queries.md), run them, and turn what they return
into something its users understand.

## Calling commands and queries

The presentation holds what is needed to run requests, the
[unit of work factory](./application/units_of_work.md#using-a-unit-of-work) and the
[event bus](./application/event_services.md#the-event-bus), and calls commands and queries with them:

```rust,ignore
let response = execute(&factory, &bus, Deposit { id, cents })?;
let accounts = ask(&factory, AccountsOf { owner, pagination: None })?;
```

It never reaches into the domain or the repositories itself: everything goes through commands and
queries.

## Translating what comes back

A request returns a [response](./application/commands_and_queries.md#the-response) or an
[error](./application/repositories.md#the-application-error), all enums of the application. The
presentation has to translate each of them into its own terms: an HTTP status and body, an exit code
and a message, a dialog.

A good practice is to write this translation once, as an implementation of a trait the presentation
understands, for:

- the application error;
- the command responses;
- the query responses.

Each place that runs a request then only has to return what the request returned.

## Example: a Rocket API

With [Rocket], that trait is [`Responder`]. Once the error and the responses implement it, a route can
return the result of a request as it is, since a `Result` of two responders is a responder too.

### The error

Each error becomes an HTTP status:

```rust,ignore
use rocket::{
    http::Status,
    response::{self, Responder},
    Request,
};

impl<'r> Responder<'r, 'static> for AppError {
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'static> {
        let status = match self {
            AppError::Domain(_) => Status::UnprocessableEntity,
            AppError::NotFound(_) => Status::NotFound,
            AppError::AlreadyExists(_) | AppError::Repository(RepositoryError::Conflict) => {
                Status::Conflict
            }
            AppError::Repository(_) | AppError::InvalidSnapshot | AppError::BusClosed => {
                Status::InternalServerError
            }
        };
        status.respond_to(request)
    }
}
```

### The command responses

```rust,ignore
impl<'r> Responder<'r, 'static> for CommandResponse {
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'static> {
        match self {
            CommandResponse::Ok => Status::Ok.respond_to(request),
            CommandResponse::Created => Status::Created.respond_to(request),
        }
    }
}
```

### The query responses

What a query found is sent as JSON, here with Rocket's `json` feature and a `serde::Serialize` derive
on `AccountSnapshot`:

```rust,ignore
use rocket::serde::json::Json;

impl<'r> Responder<'r, 'static> for AccountsResponse {
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'static> {
        match self {
            AccountsResponse::Found(accounts) => Json(accounts).respond_to(request),
            AccountsResponse::Empty => Status::NoContent.respond_to(request),
        }
    }
}
```

### The routes

The factory and the bus are kept in Rocket's managed state:

```rust,ignore
pub struct App {
    pub factory: Factory,
    pub bus: EventBus,
}
```

Each route then builds its request, runs it, and returns the result:

```rust,ignore
use rocket::{get, post, State};

#[post("/accounts/<id>/deposit/<cents>")]
fn deposit(id: u32, cents: i64, app: &State<App>) -> Result<CommandResponse, AppError> {
    execute(&app.factory, &app.bus, Deposit { id, cents })
}

#[get("/owners/<owner>/accounts")]
fn accounts_of(owner: String, app: &State<App>) -> Result<AccountsResponse, AppError> {
    ask(&app.factory, AccountsOf { owner, pagination: None })
}
```

```rust,ignore
use rocket::{launch, routes};

#[launch]
fn rocket() -> _ {
    rocket::build()
        .manage(App { factory, bus })
        .mount("/", routes![deposit, accounts_of])
}
```

Adding a route for a new command or query is then a matter of a few lines, and every route translates
errors and responses the same way.

[Rocket]: https://rocket.rs
[`Responder`]: https://api.rocket.rs/v0.5/rocket/response/trait.Responder
