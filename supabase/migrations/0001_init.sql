-- modsync v1 schema: share-code pairing, no Supabase Auth.
-- Writes go through SECURITY DEFINER RPCs only; anon has no direct
-- insert/update grant on either table.

create extension if not exists "pgcrypto";

create table profiles_owner (
  owner_id      uuid primary key default gen_random_uuid(),
  share_code    text unique not null,
  owner_secret  uuid not null default gen_random_uuid(),
  display_name  text,
  created_at    timestamptz not null default now()
);

create table synced_profiles (
  owner_id         uuid not null references profiles_owner(owner_id) on delete cascade,
  game_short_name  text not null,
  community_slug   text not null,
  profile_name     text not null,
  mods             jsonb not null,
  updated_at       timestamptz not null default now(),
  primary key (owner_id, game_short_name)
);

-- Public-safe view: never exposes owner_secret.
create view public_profiles as
  select owner_id, share_code, display_name, created_at
  from profiles_owner;

alter table profiles_owner enable row level security;
alter table synced_profiles enable row level security;

-- No policies grant insert/update/delete to anon on either base table --
-- all writes happen through the SECURITY DEFINER functions below, which
-- run as the function owner and bypass RLS internally after validating
-- the caller's owner_secret.

create policy "anon can read public_profiles view"
  on profiles_owner for select
  using (true);

create policy "anon can read synced_profiles"
  on synced_profiles for select
  using (true);

revoke all on profiles_owner from anon;
revoke all on synced_profiles from anon;
grant select on public_profiles to anon;
grant select on synced_profiles to anon;

-- Creates a new owner identity. Returns the share_code to show the user
-- and the owner_secret to persist locally (client never sees it again
-- from a plain select, since profiles_owner itself isn't grant-selectable).
create or replace function create_owner(p_share_code text, p_display_name text)
returns table(owner_id uuid, share_code text, owner_secret uuid)
language plpgsql
security definer
set search_path = public
as $$
declare
  v_owner_id uuid;
  v_owner_secret uuid;
begin
  insert into profiles_owner (share_code, display_name)
  values (p_share_code, p_display_name)
  returning profiles_owner.owner_id, profiles_owner.owner_secret
  into v_owner_id, v_owner_secret;

  return query select v_owner_id, p_share_code, v_owner_secret;
end;
$$;

revoke all on function create_owner(text, text) from public;
grant execute on function create_owner(text, text) to anon;

-- Upserts a synced profile. Requires the caller's owner_secret to match
-- the row identified by p_owner_id -- this is what prevents someone who
-- only knows a share_code (public, handed to friends) from overwriting
-- another person's synced state.
create or replace function upsert_synced_profile(
  p_owner_id uuid,
  p_owner_secret uuid,
  p_game_short_name text,
  p_community_slug text,
  p_profile_name text,
  p_mods jsonb
)
returns void
language plpgsql
security definer
set search_path = public
as $$
begin
  if not exists (
    select 1 from profiles_owner
    where owner_id = p_owner_id and owner_secret = p_owner_secret
  ) then
    raise exception 'invalid owner credentials';
  end if;

  insert into synced_profiles (owner_id, game_short_name, community_slug, profile_name, mods, updated_at)
  values (p_owner_id, p_game_short_name, p_community_slug, p_profile_name, p_mods, now())
  on conflict (owner_id, game_short_name)
  do update set
    community_slug = excluded.community_slug,
    profile_name = excluded.profile_name,
    mods = excluded.mods,
    updated_at = now();
end;
$$;

revoke all on function upsert_synced_profile(uuid, uuid, text, text, text, jsonb) from public;
grant execute on function upsert_synced_profile(uuid, uuid, text, text, text, jsonb) to anon;

-- Convenience lookup by share_code (what a friend actually types in),
-- rather than making the frontend join public_profiles -> synced_profiles
-- itself.
create or replace function get_synced_profile(p_share_code text, p_game_short_name text)
returns table(
  owner_id uuid,
  display_name text,
  community_slug text,
  profile_name text,
  mods jsonb,
  updated_at timestamptz
)
language sql
security definer
set search_path = public
stable
as $$
  select po.owner_id, po.display_name, sp.community_slug, sp.profile_name, sp.mods, sp.updated_at
  from profiles_owner po
  join synced_profiles sp on sp.owner_id = po.owner_id
  where po.share_code = p_share_code
    and sp.game_short_name = p_game_short_name;
$$;

revoke all on function get_synced_profile(text, text) from public;
grant execute on function get_synced_profile(text, text) to anon;
