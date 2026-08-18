CREATE TABLE IF NOT EXISTS public_resources (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('book', 'course', 'tool', 'article', 'podcast')),
    provider TEXT NOT NULL,
    summary TEXT NOT NULL,
    url TEXT NOT NULL,
    license TEXT NOT NULL,
    version TEXT NOT NULL,
    citation TEXT NOT NULL,
    topics TEXT[] NOT NULL DEFAULT '{}',
    status TEXT NOT NULL CHECK (status IN ('published', 'archived')),
    published_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_public_resources_published
    ON public_resources (status, updated_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_public_resources_topics
    ON public_resources USING GIN (topics);

INSERT INTO public_resources (id,title,kind,provider,summary,url,license,version,citation,topics,status,published_at)
VALUES
    ('resource-mdn-web','MDN Web Docs','article','Mozilla','面向开发者的开放 Web 平台文档与实践参考。','https://developer.mozilla.org/','CC BY-SA 2.5','2026.1','Mozilla Developer Network. MDN Web Docs. 2026.',ARRAY['学习','工具','编程'],'published','2026-01-01T00:00:00Z'),
    ('resource-ocw-learning','MIT OpenCourseWare','course','MIT','公开课程资料，适合按主题建立长期学习路径。','https://ocw.mit.edu/','CC BY-NC-SA 4.0','2026','MIT OpenCourseWare. 2026.',ARRAY['学习','课程'],'published','2026-01-01T00:00:00Z'),
    ('resource-gutenberg','Project Gutenberg','book','Project Gutenberg','可合法阅读和下载的公共领域电子书目录。','https://www.gutenberg.org/','Public Domain','2026','Project Gutenberg. 2026.',ARRAY['阅读','书籍','知识管理'],'published','2026-01-01T00:00:00Z')
ON CONFLICT (id) DO NOTHING;
