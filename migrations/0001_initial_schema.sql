CREATE EXTENSION IF NOT EXISTS btree_gist;

CREATE TABLE users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('member', 'admin')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE equipment (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    asset_tag TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 2 AND 120),
    category TEXT NOT NULL CHECK (length(trim(category)) BETWEEN 2 AND 60),
    description TEXT NOT NULL DEFAULT '' CHECK (length(description) <= 1000),
    location TEXT NOT NULL CHECK (length(trim(location)) BETWEEN 2 AND 100),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'maintenance', 'retired')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX sessions_expiry_idx ON sessions (expires_at);

CREATE TABLE reservations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    equipment_id UUID NOT NULL REFERENCES equipment(id) ON DELETE RESTRICT,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    purpose TEXT NOT NULL CHECK (length(trim(purpose)) BETWEEN 3 AND 280),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'approved', 'checked_out', 'returned', 'rejected', 'cancelled')),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT reservation_period_valid CHECK (
        starts_at < ends_at AND ends_at - starts_at <= interval '31 days'
    ),
    CONSTRAINT reservation_no_overlap EXCLUDE USING gist (
        equipment_id WITH =,
        tstzrange(starts_at, ends_at, '[)') WITH &&
    ) WHERE (status IN ('pending', 'approved', 'checked_out'))
);
CREATE INDEX reservations_user_created_idx ON reservations (user_id, created_at DESC);
CREATE INDEX reservations_equipment_period_idx ON reservations (equipment_id, starts_at, ends_at);

CREATE TABLE idempotency_keys (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    request_key TEXT NOT NULL CHECK (length(request_key) BETWEEN 8 AND 100),
    request_fingerprint TEXT NOT NULL,
    reservation_id UUID REFERENCES reservations(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, request_key)
);

CREATE TABLE audit_events (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('equipment', 'reservation')),
    entity_id UUID NOT NULL,
    actor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    action TEXT NOT NULL,
    from_state TEXT,
    to_state TEXT,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX audit_events_entity_idx ON audit_events (entity_type, entity_id, occurred_at DESC);
CREATE INDEX audit_events_time_idx ON audit_events (occurred_at DESC);

INSERT INTO users (id, email, display_name, role) VALUES
    ('00000000-0000-4000-8000-000000000001', 'member@demo.local', 'Demo Member', 'member'),
    ('00000000-0000-4000-8000-000000000002', 'admin@demo.local', 'Demo Administrator', 'admin')
ON CONFLICT (email) DO NOTHING;

INSERT INTO equipment (id, asset_tag, name, category, description, location) VALUES
    ('10000000-0000-4000-8000-000000000001', 'AV-014', '4K Conference Camera', 'Video', 'USB-C camera with auto-framing for hybrid meetings.', 'Meeting Room A'),
    ('10000000-0000-4000-8000-000000000002', 'AV-021', 'Portable Projector', 'Presentation', 'Compact laser projector with HDMI and USB-C inputs.', 'Equipment Cabinet 2F'),
    ('10000000-0000-4000-8000-000000000003', 'IT-033', 'Field Laptop', 'Computing', '14-inch laptop prepared for workshops and off-site work.', 'IT Desk'),
    ('10000000-0000-4000-8000-000000000004', 'AV-037', 'Wireless Microphone Kit', 'Audio', 'Two handheld microphones with receiver and charging dock.', 'Equipment Cabinet 2F'),
    ('10000000-0000-4000-8000-000000000005', 'NET-008', '5G Travel Router', 'Networking', 'Portable Wi-Fi router with a charging cable.', 'IT Desk'),
    ('10000000-0000-4000-8000-000000000006', 'LAB-012', 'Mirrorless Camera Kit', 'Photography', 'Camera body, standard zoom lens, battery, and SD card.', 'Studio Store')
ON CONFLICT (asset_tag) DO NOTHING;
