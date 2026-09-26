-- ============================================================
-- Clone pgmq.* into apalis_pgmq.*, patching message payload
-- columns/params/casts/types from jsonb -> bytea.
--
-- Single statement (one DO block) so this works through
-- sqlx::query()/prepared-statement execution, e.g.:
--   sqlx::query(include_str!("patch.sql")).execute(pool).await
--
-- Order of operations:
--   1. schema
--   2. composite types (pg_type, typrelid-based, e.g. message_record)
--      -- functions like read()/pop() RETURN these, so they must
--      -- exist before any function referencing them is created
--   3. meta table
--   4. functions (regex-patched). Two passes tolerate any
--      remaining forward-reference/creation-order issues.
-- ============================================================

DO $$
DECLARE
    r RECORD;
    src  text;
    patched text;
    col RECORD;
    typedef text;
    pass int;
    fail_count int;
BEGIN
    -- 0. clean slate: drop any previous (possibly broken/partial) version
    --    of this schema before rebuilding. Without this, re-running the
    --    script can layer new objects on top of stale/broken ones left
    --    over from a prior failed run.
    EXECUTE 'DROP SCHEMA IF EXISTS apalis_pgmq CASCADE';

    -- 1. schema
    EXECUTE 'CREATE SCHEMA IF NOT EXISTS apalis_pgmq';

    -- 2. composite types: any type in pgmq schema backed by a relation
    --    (i.e. CREATE TYPE ... AS (...) composite types, not enums/domains)
    FOR r IN
        SELECT t.oid AS type_oid, t.typname, c.oid AS rel_oid
        FROM pg_type t
        JOIN pg_namespace n ON n.oid = t.typnamespace
        JOIN pg_class c ON c.oid = t.typrelid
        WHERE n.nspname = 'pgmq'
          AND c.relkind = 'c'   -- composite type relation
    LOOP
        typedef := '';
        FOR col IN
            SELECT a.attname,
                   CASE
                       WHEN a.attname IN ('message', 'msg')
                            AND format_type(a.atttypid, a.atttypmod) ILIKE 'jsonb'
                       THEN 'BYTEA'
                       WHEN a.attname IN ('message', 'msg')
                            AND format_type(a.atttypid, a.atttypmod) ILIKE 'jsonb[]'
                       THEN 'BYTEA[]'
                       ELSE format_type(a.atttypid, a.atttypmod)
                   END AS coltype
            FROM pg_attribute a
            WHERE a.attrelid = r.rel_oid AND a.attnum > 0 AND NOT a.attisdropped
            ORDER BY a.attnum
        LOOP
            typedef := typedef || format('%I %s, ', col.attname, col.coltype);
        END LOOP;
        IF typedef = '' THEN
            RAISE EXCEPTION 'apalis_pgmq clone: found zero columns for pgmq.% (rel_oid=%) -- aborting rather than creating an empty composite type', r.typname, r.rel_oid;
        END IF;

        typedef := left(typedef, length(typedef) - 2); -- trim trailing ", "

        BEGIN
            EXECUTE format('DROP TYPE IF EXISTS apalis_pgmq.%I CASCADE', r.typname);
            EXECUTE format('CREATE TYPE apalis_pgmq.%I AS (%s)', r.typname, typedef);
            RAISE NOTICE 'Created type apalis_pgmq.%', r.typname;
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING 'Failed type apalis_pgmq.%: %', r.typname, SQLERRM;
        END;
    END LOOP;

    -- 3. meta table
    EXECUTE 'CREATE TABLE IF NOT EXISTS apalis_pgmq.meta (LIKE pgmq.meta INCLUDING ALL)';

    -- 3b. other base tables that exist independently of the per-queue
    --     create() flow (i.e. not q_*/a_* tables, not covered by the
    --     function-cloning loop below since they're plain CREATE TABLE
    --     statements from pgmq's install script, not wrapped in functions).
    --
    --     __pgmq_migrations is deliberately NOT cloned -- it's pgmq's own
    --     internal version-tracking table and has no bearing on this clone.
    --
    --     None of these tables carry message payloads, so no jsonb->bytea
    --     patching is needed here -- only schema-qualification.
    EXECUTE 'CREATE TABLE IF NOT EXISTS apalis_pgmq.notify_insert_throttle (
        queue_name VARCHAR NOT NULL UNIQUE REFERENCES apalis_pgmq.meta(queue_name) ON DELETE CASCADE,
        throttle_interval_ms INTEGER NOT NULL DEFAULT 0,
        last_notified_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT to_timestamp(0)
    )';

    -- 4. functions, two passes to tolerate forward-reference/ordering issues
    fail_count := 0;
    FOR pass IN 1..2 LOOP
        fail_count := 0;
        FOR r IN
            SELECT p.oid, p.proname, pg_get_functiondef(p.oid) AS def
            FROM pg_proc p
            JOIN pg_namespace n ON n.oid = p.pronamespace
            WHERE n.nspname = 'pgmq'
              AND p.prokind = 'f'
            ORDER BY p.proname
        LOOP
            src := r.def;

            -- function's own name: FUNCTION pgmq.foo -> FUNCTION apalis_pgmq.foo
            patched := regexp_replace(src, 'FUNCTION\s+pgmq\.', 'FUNCTION apalis_pgmq.', 'i');

            -- all other pgmq.<ident> references: tables, other function calls,
            -- meta, composite types (e.g. RETURNS SETOF pgmq.message_record)
            patched := regexp_replace(patched, '\mpgmq\.', 'apalis_pgmq.', 'g');

            -- quoted 'pgmq' used as a schema-name VALUE (not identifier prefix),
            -- e.g. information_schema.tables WHERE table_schema = 'pgmq'
            -- NOTE: this is broad -- it will also rewrite 'pgmq' inside
            -- human-readable error message strings (e.g. "use pgmq.create()").
            -- That's cosmetically wrong but functionally harmless; the
            -- alternative (missing real schema-comparison bugs) is worse.
            patched := regexp_replace(patched, '''pgmq''', '''apalis_pgmq''', 'g');

            -- message column type: JSONB -> BYTEA (declarations, params, returns)
            patched := regexp_replace(patched, '\m(message\s+)JSONB\M', '\1BYTEA', 'gi');
            patched := regexp_replace(patched, '\m((?:msgs?|messages?)\s+)JSONB\[\]', '\1BYTEA[]', 'gi');
            patched := regexp_replace(patched, '\m((?:msg|message)\s+)JSONB\M', '\1BYTEA', 'gi');
            patched := regexp_replace(patched, '\m(message\s+)jsonb(?=\s*[,)])', '\1bytea', 'gi');

            -- explicit ::jsonb casts on message-like identifiers
            patched := regexp_replace(patched, '\m((?:msg|message)\w*)\s*::\s*jsonb\M', '\1::bytea', 'gi');

            BEGIN
                EXECUTE patched;
                IF pass = 2 THEN
                    RAISE NOTICE 'Created apalis_pgmq.%', r.proname;
                END IF;
            EXCEPTION WHEN OTHERS THEN
                fail_count := fail_count + 1;
                IF pass = 2 THEN
                    RAISE WARNING 'Failed apalis_pgmq.% (pass %): %', r.proname, pass, SQLERRM;
                END IF;
            END;
        END LOOP;
    END LOOP;

    IF fail_count > 0 THEN
        RAISE WARNING '% function(s) failed to clone after 2 passes -- check warnings above', fail_count;
    END IF;
END;
$$;

-- ============================================================
-- Hand-written override: apalis_pgmq.read
--
-- The generic regex pass produces a `read` whose CASE statement
-- still does `message @> conditional` (jsonb containment). That
-- doesn't type-check against bytea -- Postgres type-checks BOTH
-- CASE branches regardless of which one runs, so this errors on
-- every call, even with the default conditional => '{}'.
--
-- Containment filtering on opaque bytes isn't meaningful, so this
-- override drops the conditional-filter branch entirely. If you
-- need filtering on bytea queues, filter on `headers` (still
-- jsonb) instead, or reintroduce this against your own encoding.
-- ============================================================
CREATE OR REPLACE FUNCTION apalis_pgmq.read(queue_name text, vt integer, qty integer, conditional jsonb DEFAULT '{}'::jsonb)
RETURNS SETOF apalis_pgmq.message_record
LANGUAGE plpgsql
AS $function$
DECLARE
    sql TEXT;
    qtable TEXT := apalis_pgmq.format_table_name(queue_name, 'q');
BEGIN
    IF conditional IS DISTINCT FROM '{}'::jsonb THEN
        RAISE EXCEPTION 'apalis_pgmq.read: conditional filtering on message is not supported for bytea queues (message column is bytea, not jsonb). Filter on headers instead.';
    END IF;

    sql := FORMAT(
        $QUERY$
        WITH cte AS
        (
            SELECT msg_id
            FROM apalis_pgmq.%I
            WHERE vt <= clock_timestamp()
            ORDER BY msg_id ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE apalis_pgmq.%I m
        SET
            last_read_at = clock_timestamp(),
            vt = clock_timestamp() + %L,
            read_ct = read_ct + 1
        FROM cte
        WHERE m.msg_id = cte.msg_id
        RETURNING m.*;
        $QUERY$,
        qtable, qtable, make_interval(secs => vt)
    );
    RETURN QUERY EXECUTE sql USING qty;
END;
$function$;
