-- Project names may repeat: the name is a label, and the id identifies the project.
ALTER TABLE projects DROP CONSTRAINT IF EXISTS projects_name_key;
