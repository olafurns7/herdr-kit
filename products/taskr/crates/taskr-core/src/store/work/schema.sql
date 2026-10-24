create table if not exists work_campaigns (
  root_id integer primary key references tasks(id),
  enabled integer not null default 0,
  max_active integer not null check(max_active between 1 and 64),
  max_review_pending integer not null check(max_review_pending between 1 and 64),
  workspace text,
  host text,
  no_slotr integer not null default 0
);
create table if not exists work_items (
  id integer primary key,
  root_id integer not null references tasks(id),
  title text not null,
  priority integer not null,
  revision integer not null,
  state text not null,
  current_run integer references work_runs(id),
  reason text,
  creator text not null,
  created_at text not null
);
create index if not exists work_campaign on work_items(root_id,state,priority,id);
create table if not exists work_revisions (
  work_id integer not null references work_items(id),
  revision integer not null,
  contract text,
  creator text not null,
  created_at text not null,
  primary key(work_id,revision)
);
create table if not exists work_runs (
  id integer primary key,
  work_id integer not null references work_items(id),
  revision integer not null,
  root_id integer not null references tasks(id),
  generation integer not null,
  claim_key text not null,
  claim_input text not null,
  dependencies text not null,
  round integer not null default 0,
  action text not null,
  state text not null default 'active',
  created_at text not null,
  unique(root_id,claim_key),
  foreign key(work_id,revision) references work_revisions(work_id,revision)
);
create table if not exists work_checkouts (
  host text not null,
  cwd text not null,
  run_id integer not null unique references work_runs(id),
  primary key(host,cwd)
);
create table if not exists work_assignments (
  launch_id integer primary key references launches(id),
  run_id integer not null references work_runs(id),
  stage text not null,
  round integer not null,
  lane_id integer not null references tasks(id),
  state text not null,
  process_state text not null default 'reserved',
  native_session text,
  identity_observation text,
  stop_event integer references events(id),
  lane_snapshot text not null,
  dispatch text,
  start_proof text,
  unique(run_id,stage,round)
);
create table if not exists work_results (
  id integer primary key,
  launch_id integer not null unique references work_assignments(launch_id),
  digest text not null,
  payload text not null,
  created_at text not null
);
create table if not exists work_acceptances (
  id integer primary key,
  work_id integer not null,
  revision integer not null,
  run_id integer not null references work_runs(id),
  result_id integer not null references work_results(id),
  dependencies text not null,
  created_at text not null,
  unique(work_id,revision),
  foreign key(work_id,revision) references work_revisions(work_id,revision)
);
create table if not exists work_proofs (
  result_id integer not null references work_results(id),
  checker_launch integer not null references work_assignments(launch_id),
  check_id text not null,
  payload text not null,
  primary key(checker_launch,check_id)
);
create table if not exists work_creations (
  root_id integer not null references tasks(id),
  creation_key text not null,
  input text not null,
  work_id integer not null references work_items(id),
  primary key(root_id,creation_key)
);
