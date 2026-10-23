create table if not exists jobs_operations (
  id text primary key,
  root_id integer not null references tasks(id),
  generation integer not null,
  binding text not null,
  host text not null,
  kind text not null check(kind in ('await','owner')),
  reference_id integer not null,
  revision integer not null,
  status text not null check(status in ('working','input_required','completed','failed','cancelled')),
  snapshot text,
  reply text,
  created_at text not null,
  updated_at text not null,
  unique(root_id,kind,reference_id,revision)
);
create table if not exists jobs_routes (
  operation_id text primary key references jobs_operations(id),
  state text not null check(state in ('active','released','receipted')),
  deadline text not null,
  result_id integer references work_results(id),
  run_id integer references work_runs(id),
  fallback_sent integer not null default 0 check(fallback_sent in (0,1))
);
create index if not exists jobs_route_deadline on jobs_routes(state,deadline);
create table if not exists jobs_ask_policy (
  ask_id integer primary key references events(id),
  root_id integer not null references tasks(id),
  generation integer not null,
  host text not null,
  identity text not null
);
