CREATE TABLE course_modules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    description TEXT,
    display_order INTEGER NOT NULL CHECK (display_order >= 0),
    is_published BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT course_modules_slug_not_blank CHECK (btrim(slug) <> ''),
    CONSTRAINT course_modules_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT course_modules_display_order_unique UNIQUE (display_order)
);

CREATE TABLE course_lessons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    module_id UUID NOT NULL REFERENCES course_modules (id) ON DELETE CASCADE,
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    summary TEXT,
    content TEXT NOT NULL,
    lesson_order INTEGER NOT NULL CHECK (lesson_order >= 0),
    estimated_minutes INTEGER NOT NULL DEFAULT 10 CHECK (estimated_minutes > 0),
    is_published BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT course_lessons_slug_not_blank CHECK (btrim(slug) <> ''),
    CONSTRAINT course_lessons_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT course_lessons_content_not_blank CHECK (btrim(content) <> ''),
    CONSTRAINT course_lessons_module_slug_unique UNIQUE (module_id, slug),
    CONSTRAINT course_lessons_module_order_unique UNIQUE (module_id, lesson_order)
);

CREATE TABLE course_progress (
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    lesson_id UUID NOT NULL REFERENCES course_lessons (id) ON DELETE CASCADE,
    completed BOOLEAN NOT NULL DEFAULT FALSE,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    last_accessed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (user_id, lesson_id),
    CONSTRAINT course_progress_completion_consistency CHECK (
        (completed = TRUE AND completed_at IS NOT NULL)
        OR (completed = FALSE AND completed_at IS NULL)
    )
);

CREATE INDEX course_modules_published_order_idx
    ON course_modules (display_order, id)
    WHERE is_published = TRUE;

CREATE INDEX course_lessons_module_published_order_idx
    ON course_lessons (module_id, lesson_order, id)
    WHERE is_published = TRUE;

CREATE INDEX course_progress_user_updated_at_idx
    ON course_progress (user_id, updated_at DESC);
