# Hosting the indexer API (testnet-10 beta)

The page reads lottery data from one small server, the **indexer API**. This guide explains what it is, what it
costs, and how to set it up step by step. Nothing in it needs programming.

## What the server is, and who you trust

- **It holds no keys and makes no transactions.** It reads the public testnet-10 chain through a public node,
  keeps a copy of this lottery's history on its disk (the "store"), and answers the page's questions over HTTPS.
- **The page does not trust it.** Every answer is checked in the player's browser against a public node
  (spec frontend 6.3). A wrong or dead server can stop the page from building a transaction. It cannot move
  money or make the page sign something wrong.
- **What breaks if it is down.** New visitors cannot see rounds, buy, or draw from the page. Players who already
  have a local copy can still claim and get their deposit back without it. Nothing on chain is lost.
- **Before the draw it builds the "anchor proof"** from the node, the 272-byte proof `draw()` needs (spec 6.1).
  The page checks it against the node's block before it uses it. Without it, a round can only be voided after
  24 hours.

## What runs

| Part | What it does |
|---|---|
| `indexer-api` with `--node` (one process) | follows a testnet-10 node into the store, serves `/v1/*`, builds the anchor proof for the closing round (one build at a time, kept 60 s), `/health` |
| Caddy | HTTPS with a free automatic certificate, GET only, access logs; forwards to `indexer-api` on 127.0.0.1:8790 |

**Built-in safety:**
- Per-client limits: a burst of 120 requests, then 20 a second per address, IPv6 per /64. Over that, the page
  sees "busy" and retries.
- At most 16 requests of one address at once (F6 security review), so one visitor cannot hold the server's slots.
- At most 64 requests at once and 256 connections. Requests for the anchor proof wait in their own pool of 32, so
  a flood of them during a draw leaves everything else served.
- A 10 s deadline per request; request headers at most 16 KB (Caddy).
- CORS for the site's origin only.
- GET only.
- **Two nodes must agree** before anything is written to the store (`INDEXER_API_WITNESS`, see "Which nodes the
  server reads" below).
- `/health` tells the public only "ok" or "not ok". The reasons (which can name paths and the node's address) are
  in the log, and in full only to a request from the server itself (`curl http://127.0.0.1:8790/health`).
- `/v1/status` says only `"error":"store_error"` to the public when the store stopped applying its log (F6 part
  E); the error's text (it can name the server's paths) is in the log and in a request from the server itself.

The files are in `crates/indexer-api/deploy/`:
- `Dockerfile`: the reproducible Linux build. Rust 1.94.0, Debian 12, `Cargo.lock`, the pinned rusty-kaspa
  v2.1.0. Its base images, and Caddy's in `compose.yaml`, are pinned by digest (the file says how to update one).
- `compose.yaml`: the API plus Caddy, for option A.
- `indexer-api.service`: the systemd unit, for option B.
- `Caddyfile`
- `indexer-api.env.example`: every setting, explained.

## Which nodes the server reads (an owner choice)

The server learns what happened on chain from a testnet-10 node. Players' browsers re-check every answer against
a node of their own, so **a lying node cannot make anyone lose money or sign something wrong.** What a bad node
could do is put false history into the server's copy (the store). The page then refuses the server's answers
("does not match the chain"), so Play and Rounds stop working for everybody until the store is rebuilt by hand.

What is built now (F6 security review):
- **The node is fixed for a run.** With `INDEXER_API_NODE=resolver`, the first public node the resolver gives is
  kept; reconnects go back to it, never to a new one.
- **A second node must agree** (`INDEXER_API_WITNESS=resolver`, the default in `indexer-api.env.example`). Before
  each batch is written, another public node is asked the same question. Only what both report the same is written.
  If they report the same block with different contents, nothing is written and `/health` says "not ok" at once. If
  the second node just has not seen the blocks yet, the server waits; after 2 minutes without progress `/health`
  says "not ok". So one bad node can make the server stop and wait, but it can no longer write into the store.

| Option | Who you trust | Money at risk | Work for you | Monthly cost |
|---|---|---|---|---|
| **A. Two public nodes must agree (built, recommended for the beta)** | the two node operators not to lie *together*; the public resolver to hand out two different nodes (on 2026-10-27 it handed out three) | none (browsers re-check everything) | none: it is the default setting | nothing extra; the server reads twice as much from public nodes |
| **B. Your own testnet-10 node on the server** (`INDEXER_API_NODE=ws://127.0.0.1:<port>`, and a public node as the witness) | yourself; the witness still double-checks | none | install and keep a Kaspa node running and up to date; a restart takes time to catch up | a bigger server: a node keeps a lot of chain data in memory and on disk. We have not measured a testnet-10 node; our private test node grew to about 6.6 GB of memory after 864,000 blocks (about one day of testnet-10). Plan for a 16 GB server with a large SSD, several times the €6 plan; we would measure it before you pay |
| **C. One public node, unchecked** (`INDEXER_API_WITNESS=off`) | that one node operator | none | none | nothing extra, but a bad node can block the page until the store is rebuilt (an hour of work: re-index from the start block) |

If the two public nodes keep disagreeing or one is down for long, the page shows the API as unavailable; players
with a local copy can still claim. The fix is to wait, or to set another witness URL, or option B.

## Choose a host (prices checked 2026-10-27, before VAT)

| Option | Plan | Size | Price a month | Notes | Source |
|---|---|---|---|---|---|
| **1. Hetzner Cloud** (Germany/Finland) | CX23 | 2 vCPU, 4 GB, 40 GB | **€5.99** (€5.49 + €0.50 IPv4) | Cheapest. These plans were often **out of stock** in Sept–Oct 2026. Prices rose on 15 June 2026. | [Hetzner cost-optimized](https://www.hetzner.com/cloud/cost-optimized/), [price change](https://docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/), [IPv4](https://docs.hetzner.com/general/infrastructure-and-availability/ipv4-pricing) |
| **2. DigitalOcean** | Basic Droplet 2 GB | 1 vCPU, 2 GB, 50 GB | **$12** (the 1 GB plan is $6) | Simplest control panel. IPv4 is included. Tax is added by country. | [DigitalOcean droplets](https://www.digitalocean.com/pricing/droplets) |
| **3. Fly.io** (container service, no server to manage) | shared-cpu-1x, 1 GB, plus a 5 GB volume | shared CPU | **about $7.45** ($6.70 + $0.75 volume; a dedicated IPv4 is $2 more and optional) | No free tier for new accounts. Needs its own config: we have not written or tested it. | [Fly.io pricing](https://docs.fly.io/about/pricing) |

**Recommendation:** option 1 if Hetzner has stock, else option 2. Measured on 2026-10-27, the server uses about
**108 MB of memory**: the Linux container on the M8 store (37 MB log, 9 rounds), following a public node.

- **Any of these plans can run it.** Building it is heavier: the Docker build took 5 minutes on the PC (2 jobs);
  its memory was not measured.
- **On a 1–2 GB plan**, build on your PC (option B) and copy the binary over.
- **Option A** builds on the server: use it on a 4 GB plan such as Hetzner's CX23.

**Monthly total for the beta:**
- €6–12 for the server.
- The API's domain: a subdomain of a domain you already own costs nothing extra.
- Vercel Hobby for the site: free while the beta is non-commercial (no payments, no ads, nobody paid to build it).
  Vercel Pro is $20 a month ([Vercel fair use](https://vercel.com/docs/limits/fair-use-guidelines),
  [pricing](https://vercel.com/pricing)).

## Steps (about an hour the first time)

You need:
- the server, running Ubuntu 24.04 or Debian 12;
- a domain name for the API (for example `api.yourdomain.com`);
- the beta instance's directory from your PC: `tn10-data/beta/<covenant ID>/`, written by `tn10 beta genesis`
  (docs/launch/beta-plan.md).

1. **DNS.** At your domain provider, add an **A record** `api` pointing to the server's IPv4 address. Caddy needs
   it to get the certificate, and ports 80 and 443 must be open
   ([Caddy automatic HTTPS](https://caddyserver.com/docs/automatic-https)).
2. **Log in to the server** (the provider's web console or `ssh root@<ip>`) and update it:
   `apt update && apt upgrade -y`.
3. **Firewall.** Allow only SSH, HTTP and HTTPS:
   `ufw allow OpenSSH && ufw allow 80 && ufw allow 443 && ufw enable`.
4. **Copy the store** from your PC. In PowerShell on the PC:
   `scp -r tn10-data\beta\<covenant ID> root@<ip>:/root/store`.
   It holds `index-log.jsonl`, `index.json` and `bundle.json`. The server resumes from them, so it needs no
   `--start`.
5. **Install and start.** Choose A or B.

### A. Docker (recommended on a 2 GB+ server)

1. Install Docker: `curl -fsSL https://get.docker.com | sh`.
2. Get the code: `git clone <the repository URL> kaspalionare && cd kaspalionare/crates/indexer-api/deploy`.
3. Write the settings:
   - `cp indexer-api.env.example indexer-api.env`, then `nano indexer-api.env`;
   - `INDEXER_API_COV` = the covenant ID;
   - `INDEXER_API_ALLOW_ORIGIN` = the site's address, for example `https://kaspalionare.vercel.app`, plus your
     own domain if the site has one, comma-separated;
   - leave `INDEXER_API_NODE=resolver` and `INDEXER_API_WITNESS=resolver`, or put nodes' `wss://` URLs (see "Which
     nodes the server reads").
4. In `Caddyfile`, replace `api.example.com` with the API's domain.
5. Move the store into place: `mkdir -p data && cp -r /root/store/. data/ && chown -R 10001 data`.
6. Start: `docker compose up -d --build`. The first build takes about 20–40 minutes.
7. Logs: `docker compose logs -f`. Stop: `docker compose down`. It finishes the current batch first, and the store
   stays in `./data`.

### B. Binary and systemd (for a 1–2 GB server)

1. On the PC (Docker Desktop running), from the repository root:
   `docker build -f crates/indexer-api/deploy/Dockerfile --target export --output type=local,dest=out .`
   This writes `out/indexer-api`, a Linux x86-64 binary.
2. Copy it and the files to the server:
   `scp out/indexer-api crates/indexer-api/deploy/indexer-api.service crates/indexer-api/deploy/indexer-api.env.example crates/indexer-api/deploy/Caddyfile root@<ip>:/root/`.
3. On the server, install the binary and its settings:
   - `install -m 755 /root/indexer-api /usr/local/bin/indexer-api`;
   - `mkdir -p /etc/kaspalionare && cp /root/indexer-api.env.example /etc/kaspalionare/indexer-api.env`;
   - in `/etc/kaspalionare/indexer-api.env`: set `INDEXER_API_COV`, `INDEXER_API_ALLOW_ORIGIN`, and
     `INDEXER_API_STORE=/var/lib/kaspalionare`.
4. Install the store and the service:
   - `mkdir -p /var/lib/private/kaspalionare && cp -r /root/store/. /var/lib/private/kaspalionare/`;
   - `cp /root/indexer-api.service /etc/systemd/system/`;
   - `systemctl daemon-reload && systemctl enable --now indexer-api`.
5. Logs: `journalctl -u indexer-api -f`.
6. Caddy:
   - install it from the official apt repository
     ([Caddy install](https://caddyserver.com/docs/install), "Debian, Ubuntu");
   - `cp /root/Caddyfile /etc/caddy/Caddyfile`, then replace the domain in it;
   - `systemctl reload caddy`.

## Check it works

1. `https://api.yourdomain.com/health` shows `{"ok":true}`. On the server itself,
   `curl http://127.0.0.1:8790/health` shows the detail: `"following":true`, the node it follows and the witness.
2. `https://api.yourdomain.com/v1/status`: its `lastChainDaaScore` grows every few seconds, and
   `covenantId` is the instance's.
3. `https://api.yourdomain.com/v1/instance`: its blake2b-256 is the page's `VITE_BUNDLE_HASH`.
   `tn10 beta genesis` printed it.
4. In the page, Rounds lists round 1, and Verify in this browser shows matches.
5. Set a free uptime monitor (for example UptimeRobot) on `/health`. It answers 503 when:
   - the node has not been read for 2 minutes, or the witness has confirmed nothing new for 2 minutes;
   - the two nodes report one block differently (at once);
   - the follower hit an error a retry cannot fix (a pruned resume block);
   - the store does not apply.

## Run it

- **Restarts are safe.** Every batch is written to the store before the next node call. A stop, a crash or a reboot
  resumes from the log.
- **Node trouble** does not stop the API. It retries (5 s doubling to 2 min), reconnects to the same node after two
  failures in a row, and `/health` turns 503; the reason is in the log (`docker compose logs` or `journalctl`).
  A restart may pick other public nodes.
- **Seen on 2026-10-27:** the M8 store's resume block (DAA 605,601,500, two days old) was no longer on the
  public node. The follower reported it, kept serving the store, and stopped cleanly on SIGTERM.
- **Do not leave it off for days.** If the server is off longer than the node keeps old blocks, it cannot resume:
  `/health` shows "below its retention root". `tn10 node-check` prints how far back the node keeps blocks. The fix
  is a re-anchor of the store (`tn10 campaign reanchor`, M8). Today that tool works only on the M8 instance's
  directory; a beta option for it is still to be written (docs/specs/frontend-handoff.md, "F6 part C").
- **Disk:** the M8 store grew to 37 MB in about 9 rounds. 20 GB is plenty.
- **Updates:**
  - option A: `git pull && docker compose up -d --build`;
  - option B: build and copy the binary again, then `systemctl restart indexer-api`.

## Behind the Vercel proxy (proposal, not tested; docs/launch/site-deploy.md)

If the site forwards `/api/v1/*` to this server (no domain for the server), every request arrives from one of
Vercel's addresses, which are not published. Vercel puts the visitor's address in `X-Real-IP` (and overwrites
`X-Forwarded-For`), but anyone can send those headers straight to the server's IP. **Proposed rule:**

1. A long random secret, set in two places only: Vercel's project settings (an environment variable that a
   `routes` transform copies into a request header, `X-Origin-Secret`, as Vercel documents under "Restricting your
   origin to Vercel traffic") and the server's Caddy environment (`ORIGIN_SECRET`).
2. Caddy (the only thing listening on the internet) passes a request to the API **only when the header matches**,
   and then gives the API the visitor's address from `X-Real-IP`; everything else gets 403. The API keeps its
   rule: it trusts `X-Forwarded-For` only from Caddy on the same machine (`INDEXER_API_CLIENT_IP=forwarded`).
   Sketch, to be checked with a local Caddy first (Caddy's docs do not say whether `header_up` can replace the
   `X-Forwarded-For` it sets itself; [reverse_proxy](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy)):

   ```
   :80 {
     @vercel header X-Origin-Secret {$ORIGIN_SECRET}
     handle @vercel {
       reverse_proxy 127.0.0.1:8790 {
         header_up X-Forwarded-For {http.request.header.X-Real-IP}
       }
     }
     respond 403
   }
   ```
3. Over plain http (an IP-only server) the secret crosses the network unencrypted. Whoever reads it can only
   pretend to be many visitors (dodging the per-client limits); answers they change are refused by the page. No
   money is at risk. A domain with https for the server closes this.

## Per-client limits behind a different proxy

`indexer-api` trusts `X-Forwarded-For` only from a proxy on the same machine (`INDEXER_API_CLIENT_IP=forwarded`, a
loopback peer). With the server facing the internet directly, set `INDEXER_API_CLIENT_IP=peer`. With a proxy on
another machine, its address is the client for every request. One bucket for everybody would throttle the page, so
in that case set `INDEXER_API_PER_CLIENT_RATE=off` and let the proxy limit per client.
