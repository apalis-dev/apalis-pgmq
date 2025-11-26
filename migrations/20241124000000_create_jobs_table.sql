CREATE TABLE IF NOT EXISTS jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_type VARCHAR(255) NOT NULL,
    args JSONB NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
    run_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    done_at TIMESTAMPTZ,
    lock_at TIMESTAMPTZ,
    lock_by VARCHAR(255),
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_jobs_type_status ON jobs(job_type, status);

CREATE INDEX IF NOT EXISTS idx_jobs_run_at ON jobs(run_at) WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_jobs_lock_at ON jobs(lock_at) WHERE lock_at IS NOT NULL;
