<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";

  type LocalProfile = {
    game_short_name: string;
    profile_name: string;
    path: string;
  };

  type VersionNumber = { major: number; minor: number; patch: number };

  type ModEntry = {
    name: string;
    authorName: string;
    displayName: string;
    versionNumber: VersionNumber;
    enabled: boolean;
  };

  type ProfileSummary = {
    profile: LocalProfile;
    community_slug: string | null;
    mods: ModEntry[];
  };

  type Friend = { share_code: string; nickname: string };

  type SyncPlanEntry = {
    full_name: string;
    display_name: string;
    author_name: string;
    local_version: string | null;
    friend_version: string | null;
  };

  type SyncPlan = {
    to_install: SyncPlanEntry[];
    to_update: SyncPlanEntry[];
    to_disable: SyncPlanEntry[];
    to_reenable: SyncPlanEntry[];
    unchanged: SyncPlanEntry[];
  };

  let profiles = $state<LocalProfile[]>([]);
  let selected = $state<ProfileSummary | null>(null);
  let error = $state("");
  let loading = $state(false);

  let sharing = $state(false);
  let shareCode = $state<string | null>(null);
  let shareError = $state("");

  let friends = $state<Friend[]>([]);
  let newFriendCode = $state("");
  let newFriendNickname = $state("");
  let friendsError = $state("");

  let diffing = $state(false);
  let syncPlan = $state<SyncPlan | null>(null);
  let syncPlanError = $state("");
  let syncPlanFriend = $state<Friend | null>(null);

  type SyncProgressEvent = { step: string; current: number; total: number };
  type SyncSummary = {
    installed_or_updated: number;
    disabled: number;
    reenabled: number;
  };

  let syncing = $state(false);
  let syncProgress = $state<SyncProgressEvent | null>(null);
  let syncSummary = $state<SyncSummary | null>(null);
  let syncExecError = $state("");

  listen<SyncProgressEvent>("sync-progress", (event) => {
    syncProgress = event.payload;
  });

  let pendingUpdate = $state<Update | null>(null);
  let updateInstalling = $state(false);
  let updateError = $state("");

  async function checkForUpdate() {
    try {
      pendingUpdate = await check();
    } catch (e) {
      // Don't nag the user if the check itself fails (e.g. offline) --
      // this runs silently on every launch, so a network hiccup shouldn't
      // surface as an error banner.
      console.error("update check failed", e);
    }
  }

  async function installUpdate() {
    if (!pendingUpdate) return;
    updateInstalling = true;
    updateError = "";
    try {
      await pendingUpdate.downloadAndInstall();
      await relaunch();
    } catch (e) {
      updateError = String(e);
      updateInstalling = false;
    }
  }

  async function loadProfiles() {
    error = "";
    loading = true;
    try {
      profiles = await invoke<LocalProfile[]>("list_profiles");
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function selectProfile(p: LocalProfile) {
    error = "";
    selected = null;
    shareCode = null;
    shareError = "";
    syncPlan = null;
    syncPlanError = "";
    syncPlanFriend = null;
    try {
      selected = await invoke<ProfileSummary>("get_profile_summary", {
        profilePath: p.path,
      });
    } catch (e) {
      error = String(e);
    }
  }

  async function shareProfile() {
    if (!selected) return;
    sharing = true;
    shareError = "";
    shareCode = null;
    try {
      shareCode = await invoke<string>("push_profile", {
        profilePath: selected.profile.path,
      });
    } catch (e) {
      shareError = String(e);
    } finally {
      sharing = false;
    }
  }

  async function loadFriends() {
    friendsError = "";
    try {
      friends = await invoke<Friend[]>("list_friends");
    } catch (e) {
      friendsError = String(e);
    }
  }

  async function addFriend(event: Event) {
    event.preventDefault();
    if (!newFriendCode.trim() || !newFriendNickname.trim()) return;
    friendsError = "";
    try {
      friends = await invoke<Friend[]>("add_friend", {
        shareCode: newFriendCode.trim(),
        nickname: newFriendNickname.trim(),
      });
      newFriendCode = "";
      newFriendNickname = "";
    } catch (e) {
      friendsError = String(e);
    }
  }

  async function removeFriend(code: string) {
    friendsError = "";
    try {
      friends = await invoke<Friend[]>("remove_friend", { shareCode: code });
    } catch (e) {
      friendsError = String(e);
    }
  }

  async function previewSync(friend: Friend) {
    if (!selected) return;
    diffing = true;
    syncPlanError = "";
    syncPlan = null;
    syncSummary = null;
    syncExecError = "";
    syncPlanFriend = friend;
    try {
      syncPlan = await invoke<SyncPlan>("fetch_friend_diff", {
        profilePath: selected.profile.path,
        friendShareCode: friend.share_code,
      });
    } catch (e) {
      syncPlanError = String(e);
    } finally {
      diffing = false;
    }
  }

  async function runSync() {
    if (!selected || !syncPlanFriend) return;
    syncing = true;
    syncExecError = "";
    syncSummary = null;
    syncProgress = null;
    try {
      syncSummary = await invoke<SyncSummary>("execute_sync", {
        profilePath: selected.profile.path,
        friendShareCode: syncPlanFriend.share_code,
      });
      // Refresh the mod list so the view reflects what's now on disk.
      selected = await invoke<ProfileSummary>("get_profile_summary", {
        profilePath: selected.profile.path,
      });
      syncPlan = null;
    } catch (e) {
      syncExecError = String(e);
    } finally {
      syncing = false;
      syncProgress = null;
    }
  }

  loadProfiles();
  loadFriends();
  checkForUpdate();
</script>

<main class="container">
  <h1>modsync</h1>
  <p class="subtitle">Detected r2modman profiles on this machine</p>

  {#if pendingUpdate}
    <div class="update-banner">
      <span>Update available: v{pendingUpdate.version}</span>
      <button onclick={installUpdate} disabled={updateInstalling}>
        {updateInstalling ? "Updating..." : "Update & restart"}
      </button>
      {#if updateError}
        <span class="error">{updateError}</span>
      {/if}
    </div>
  {/if}

  {#if loading}
    <p>Scanning...</p>
  {:else if error}
    <p class="error">{error}</p>
  {:else if profiles.length === 0}
    <p>No r2modman profiles found.</p>
  {:else}
    <ul class="profile-list">
      {#each profiles as p}
        <li>
          <button class="profile-btn" onclick={() => selectProfile(p)}>
            <strong>{p.game_short_name}</strong> / {p.profile_name}
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if selected}
    <section class="summary">
      <h2>
        {selected.profile.game_short_name} / {selected.profile.profile_name}
      </h2>
      <p class="slug">community: {selected.community_slug ?? "unknown"}</p>

      <div class="share-row">
        <button onclick={shareProfile} disabled={sharing}>
          {sharing ? "Sharing..." : "Share this profile"}
        </button>
        {#if shareCode}
          <span class="share-code">Your code: <strong>{shareCode}</strong></span>
        {/if}
        {#if shareError}
          <span class="error">{shareError}</span>
        {/if}
      </div>

      <table>
        <thead>
          <tr>
            <th>Mod</th>
            <th>Version</th>
            <th>Enabled</th>
          </tr>
        </thead>
        <tbody>
          {#each selected.mods as m}
            <tr class:disabled={!m.enabled}>
              <td>{m.displayName} <span class="author">by {m.authorName}</span></td>
              <td
                >{m.versionNumber.major}.{m.versionNumber.minor}.{m
                  .versionNumber.patch}</td
              >
              <td>{m.enabled ? "yes" : "no"}</td>
            </tr>
          {/each}
        </tbody>
      </table>

      {#if friends.length > 0}
        <div class="sync-picker">
          <h3>Sync from a friend</h3>
          <ul class="friend-list">
            {#each friends as f}
              <li>
                <span
                  ><strong>{f.nickname}</strong>
                  <span class="muted">{f.share_code}</span></span
                >
                <button onclick={() => previewSync(f)} disabled={diffing}>
                  {diffing && syncPlanFriend?.share_code === f.share_code
                    ? "Comparing..."
                    : "Preview sync"}
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if syncPlanError}
        <p class="error">{syncPlanError}</p>
      {/if}

      {#if syncPlan && syncPlanFriend}
        <div class="sync-plan">
          <h3>Plan: sync from {syncPlanFriend.nickname}</h3>

          {#if syncPlan.to_install.length === 0 && syncPlan.to_update.length === 0 && syncPlan.to_disable.length === 0 && syncPlan.to_reenable.length === 0}
            <p class="muted">Already in sync with this friend.</p>
          {/if}

          {#if syncPlan.to_install.length > 0}
            <p class="plan-group-title">Install ({syncPlan.to_install.length})</p>
            <ul class="plan-list">
              {#each syncPlan.to_install as e}
                <li>{e.display_name} <span class="muted">{e.friend_version}</span></li>
              {/each}
            </ul>
          {/if}

          {#if syncPlan.to_update.length > 0}
            <p class="plan-group-title">Update ({syncPlan.to_update.length})</p>
            <ul class="plan-list">
              {#each syncPlan.to_update as e}
                <li>
                  {e.display_name}
                  <span class="muted">{e.local_version} &rarr; {e.friend_version}</span>
                </li>
              {/each}
            </ul>
          {/if}

          {#if syncPlan.to_reenable.length > 0}
            <p class="plan-group-title">Re-enable ({syncPlan.to_reenable.length})</p>
            <ul class="plan-list">
              {#each syncPlan.to_reenable as e}
                <li>{e.display_name}</li>
              {/each}
            </ul>
          {/if}

          {#if syncPlan.to_disable.length > 0}
            <p class="plan-group-title">Disable ({syncPlan.to_disable.length})</p>
            <ul class="plan-list">
              {#each syncPlan.to_disable as e}
                <li>{e.display_name}</li>
              {/each}
            </ul>
          {/if}

          <div class="sync-actions">
            <button onclick={runSync} disabled={syncing}>
              {syncing ? "Syncing..." : "Sync now"}
            </button>
            {#if syncing && syncProgress}
              <span class="muted"
                >{syncProgress.step} ({syncProgress.current}/{syncProgress.total})</span
              >
            {/if}
          </div>

          {#if syncExecError}
            <p class="error">{syncExecError}</p>
          {/if}

          {#if syncSummary}
            <p class="sync-success">
              Done &mdash; {syncSummary.installed_or_updated} installed/updated,
              {syncSummary.disabled} disabled, {syncSummary.reenabled} re-enabled.
            </p>
          {/if}
        </div>
      {/if}
    </section>
  {/if}

  <section class="friends">
    <h2>Friends</h2>
    {#if friendsError}
      <p class="error">{friendsError}</p>
    {/if}
    <form class="add-friend-row" onsubmit={addFriend}>
      <input placeholder="Their share code" bind:value={newFriendCode} />
      <input placeholder="Nickname" bind:value={newFriendNickname} />
      <button type="submit">Add</button>
    </form>
    {#if friends.length === 0}
      <p class="muted">No friends added yet.</p>
    {:else}
      <ul class="friend-list">
        {#each friends as f}
          <li>
            <span><strong>{f.nickname}</strong> <span class="muted">{f.share_code}</span></span>
            <button class="remove-btn" onclick={() => removeFriend(f.share_code)}>remove</button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</main>

<style>
  :root {
    font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
    color: #0f0f0f;
    background-color: #f6f6f6;
  }

  .container {
    margin: 0 auto;
    max-width: 720px;
    padding: 3rem 1.5rem;
  }

  h1 {
    margin-bottom: 0;
  }

  .subtitle {
    color: #666;
    margin-top: 0.25rem;
  }

  .error {
    color: #c0392b;
  }

  .update-banner {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-top: 1rem;
    padding: 0.6rem 1rem;
    border-radius: 8px;
    background: #eaf2ff;
    border: 1px solid #b9d3ff;
    font-size: 0.9rem;
  }

  .profile-list {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .profile-btn {
    width: 100%;
    text-align: left;
    padding: 0.75rem 1rem;
    border-radius: 8px;
    border: 1px solid #ddd;
    background: #fff;
    cursor: pointer;
  }

  .profile-btn:hover {
    border-color: #396cd8;
  }

  .summary {
    margin-top: 2rem;
  }

  .slug {
    color: #666;
    font-size: 0.9rem;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    margin-top: 1rem;
  }

  th,
  td {
    text-align: left;
    padding: 0.4rem 0.5rem;
    border-bottom: 1px solid #e5e5e5;
    font-size: 0.9rem;
  }

  tr.disabled {
    opacity: 0.5;
  }

  .author {
    color: #888;
    font-size: 0.8rem;
  }

  .share-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-top: 0.75rem;
  }

  .share-code {
    font-size: 0.9rem;
  }

  .sync-picker,
  .sync-plan {
    margin-top: 1.5rem;
    padding-top: 1rem;
    border-top: 1px solid #e5e5e5;
  }

  .sync-picker h3,
  .sync-plan h3 {
    margin: 0 0 0.5rem 0;
    font-size: 1rem;
  }

  .sync-picker .friend-list li {
    padding: 0.4rem 0.75rem;
  }

  .plan-group-title {
    font-weight: 600;
    margin: 0.75rem 0 0.25rem 0;
    font-size: 0.9rem;
  }

  .plan-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.9rem;
  }

  .plan-note {
    margin-top: 1rem;
    font-style: italic;
  }

  .sync-actions {
    margin-top: 1rem;
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .sync-success {
    color: #2e7d32;
    font-size: 0.9rem;
    margin-top: 0.5rem;
  }

  .friends {
    margin-top: 2.5rem;
    border-top: 1px solid #e5e5e5;
    padding-top: 1.5rem;
  }

  .add-friend-row {
    display: flex;
    gap: 0.5rem;
    margin: 0.75rem 0;
  }

  .add-friend-row input {
    flex: 1;
    border-radius: 8px;
    border: 1px solid #ddd;
    padding: 0.5rem 0.75rem;
  }

  .friend-list {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .friend-list li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem 0.75rem;
    border-radius: 8px;
    border: 1px solid #ddd;
  }

  .remove-btn {
    font-size: 0.8rem;
    padding: 0.25rem 0.6rem;
  }

  .muted {
    color: #888;
    font-size: 0.85rem;
  }

  @media (prefers-color-scheme: dark) {
    :root {
      color: #f6f6f6;
      background-color: #2f2f2f;
    }

    .profile-btn {
      background: #1f1f1f;
      border-color: #3a3a3a;
      color: #f6f6f6;
    }

    th,
    td {
      border-bottom-color: #3a3a3a;
    }

    .subtitle,
    .slug,
    .author,
    .muted {
      color: #aaa;
    }

    .friends,
    .sync-picker,
    .sync-plan {
      border-top-color: #3a3a3a;
    }

    .add-friend-row input,
    .friend-list li {
      background: #1f1f1f;
      border-color: #3a3a3a;
      color: #f6f6f6;
    }

    .update-banner {
      background: #1c2a40;
      border-color: #2f4a70;
      color: #f6f6f6;
    }
  }
</style>
