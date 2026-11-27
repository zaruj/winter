# winter

Desktop app: Rust core + Tauri v2 GUI (SvelteKit, Tailwind CSS, shadcn-svelte).

## Features

- **Kubernetes**: pick a context from your kubeconfig, tick namespaces, then browse (from a
  sidebar grouped into Cluster, Workloads, Network, Storage and Config)
  deployments, pods, services, ingresses, PVCs, config maps and secrets (names, types and key
  counts only; never values) and nodes. Lists load in pages and show rows as they arrive, then
  stay live: pods and everything else update as the cluster changes (a watch, as in
  `kubectl get -w`), with no refresh needed. Open a node for its conditions, capacity, taints,
  labels and pods (on one screen, no scrolling), plus CPU and memory use when the cluster runs metrics-server. Follow pods'
  logs and open shells in them (`kubectl exec -it`, bash if present, else sh) in tabs docked
  below the list. Press `:` to open
  any other kind the cluster serves (nodes, statefulsets, jobs, CRDs, ...) by name or short
  name, as in k9s; it shows the same columns as `kubectl get`.
- **Terminal**: your login shell in tabs, with the font and colours of Terminal.app's default
  profile. Shells keep running when you leave the page.
- **AWS**: pick a profile from `~/.aws` (access keys, SSO, assumed roles and credential
  processes all work, as with the AWS CLI) and a region, then list EC2 instances, EKS clusters
  and load balancers (application, network, gateway and classic). Press `:` to jump to a
  service or region by name, and `Enter` on an instance for its details (status checks, AMI,
  network, security groups, volumes, tags) on one screen. Read-only.
- SSH, Azure, Google Cloud, Cloudflare, Akamai: not built yet.

## Keyboard

Press `?` in the app for this list.

| Where | Keys |
| --- | --- |
| Everywhere | `Esc` back (outside the terminal), `⌘[` back, `?` help |
| Home | `k` Kubernetes, `t` Terminal, `a` AWS, arrows + `Enter` |
| Clusters | type to search, `↑` `↓` or `Ctrl+K` `Ctrl+J` move, `Enter` open, `Esc` clear search |
| Cluster | `:` any resource (`:sts`, `:crd`, `:po argocd`, `:svc all`), `j` `k` move, `g` `G` top/bottom, `1`–`9` tabs (`9` Nodes), `[` `]` previous/next in the sidebar, `Space` tick namespace, `Enter` open namespace or node details, `0` all namespaces, `l` pod logs (docked below), `L` show and focus / hide logs and shells, `⌘J` switch between list and dock, `s` pod shell (docked below), `/` filter, `r` refresh |
| Node details | `j` `k` move in the node's pods (paged to fit), `l` or `Enter` logs, `s` shell, `r` refresh, `Esc` back to the nodes |
| Shell (docked) | keys go to the shell, `⌘J` back to the list, `⌘W` or `exit` closes the tab |
| Logs (focused) | `j` `k` scroll, `u` `d` page, `g` `G` top/bottom, `[` `]` switch log, `x` or `⌘W` close, `s` shell, `Esc` back to the list |
| AWS | type to search profiles, `Enter` open, `:` service or region (`:eks`, `:alb us-east-1`), `Enter` instance details, `1`–`3` EC2 / EKS / load balancers, `[` `]` switch, `j` `k` move, `/` filter, `r` refresh |
| Terminal | `⌘T` new tab, `⌘W` close tab, `⌘1`–`⌘9` switch, `⌘[` leave (`Esc` goes to the shell) |

## Layout

| Path | What it is |
| --- | --- |
| `crates/winter-core` | Application logic. Plain Rust, no Tauri dependency. |
| `src-tauri` | Tauri shell. Commands only forward to `winter-core`. |
| `src` | Svelte UI: `src/routes/{k8s,terminal}`, shared code in `src/lib`. Shortcuts: `src/lib/keys.svelte.ts`. |

To add a feature: write it in `winter-core`, expose it in `src-tauri/src/commands.rs`, register it in
`src-tauri/src/lib.rs`, then add a typed wrapper (e.g. `src/lib/k8s/api.ts`).

Long lists stream: the command takes a Tauri `Channel`, sends each page as it arrives and returns
the total. In the UI, `ResourceList` appends pages, or keeps current rows on screen until a reload
finishes.

## Requirements

Node 22.17+, stable Rust, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

## Commands

```sh
npm install
npm run tauri dev                  # run the app
npm run tauri build                # production bundle
npm run check                      # type-check the UI
cargo test --workspace             # test Rust
WINTER_TEST_CONTEXT=<context> cargo test -p winter-core --test live_cluster -- --ignored
                                   # test against a real cluster: lists, watches, reads logs;
                                   # creates and deletes ConfigMap default/winter-watch-test;
                                   # with WINTER_TEST_POD=<ns>/<pod> also runs `echo` in that pod
WINTER_TEST_AWS_PROFILE=<profile> cargo test -p winter-core --test live_aws -- --ignored
                                   # test against a real AWS account (read-only calls)
cargo clippy --workspace --all-targets
npx shadcn-svelte@latest add dialog   # add a UI component
```
