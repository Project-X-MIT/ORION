-- Development-only identities. Authentication setup may replace these
-- deliberately unusable hashes; production must never execute this seed.
INSERT INTO users (id, email, username, password_hash, display_name)
VALUES
    (
        '00000000-0000-0000-0000-000000000101',
        'admin@orion.local',
        'orion_admin',
        '!development-seed-requires-password-reset!',
        'ORION Admin'
    ),
    (
        '00000000-0000-0000-0000-000000000102',
        'learner@orion.local',
        'orion_learner',
        '!development-seed-requires-password-reset!',
        'ORION Learner'
    )
ON CONFLICT (id) DO UPDATE SET
    email = EXCLUDED.email,
    username = EXCLUDED.username,
    display_name = EXCLUDED.display_name;

INSERT INTO user_ratings (user_id)
VALUES
    ('00000000-0000-0000-0000-000000000101'),
    ('00000000-0000-0000-0000-000000000102')
ON CONFLICT (user_id) DO NOTHING;
