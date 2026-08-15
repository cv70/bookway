import { Archive, BookOpenText, Check, ChevronRight, Clock3, ExternalLink, Plus, RotateCcw, X } from 'lucide-react-native';
import { useEffect, useMemo, useState } from 'react';
import {
  KeyboardAvoidingView,
  ActivityIndicator,
  Linking,
  Modal,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { colors } from '../theme';
import { CreateKnowledgeResourceInput, KnowledgeResource, KnowledgeResourceKind, KnowledgeResourceStatus, ReadingBook, ReadingBookmark, UpdateKnowledgeResourceInput } from '../types';

type Props = {
  books: ReadingBook[];
  bookmarks: ReadingBookmark[];
  resources: KnowledgeResource[];
  visible: boolean;
  onClose: () => void;
  onOpenBook: (book: ReadingBook) => void;
  onCreateResource: (input: CreateKnowledgeResourceInput) => Promise<void>;
  onStartJourney: (resource: KnowledgeResource) => Promise<void>;
  onUpdateResource: (resourceId: string, input: UpdateKnowledgeResourceInput) => Promise<void>;
};

export function ReadingLibraryModal({ books, bookmarks, resources, visible, onClose, onOpenBook, onCreateResource, onStartJourney, onUpdateResource }: Props) {
  const [adding, setAdding] = useState(false);
  const [title, setTitle] = useState('');
  const [author, setAuthor] = useState('');
  const [content, setContent] = useState('');
  const [sourceUrl, setSourceUrl] = useState('');
  const [tags, setTags] = useState('');
  const [kind, setKind] = useState<KnowledgeResourceKind>('book');
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState(false);
  const [sourceUrlError, setSourceUrlError] = useState(false);
  const [activatingResourceId, setActivatingResourceId] = useState<string>();
  const [activationErrorId, setActivationErrorId] = useState<string>();
  const [resourceFilter, setResourceFilter] = useState<KnowledgeResourceStatus | 'all'>('all');
  const [resourceQuery, setResourceQuery] = useState('');

  useEffect(() => {
    if (!visible) return;
    setAdding(false);
    setTitle('');
    setAuthor('');
    setContent('');
    setSourceUrl('');
    setTags('');
    setKind('book');
    setCreating(false);
    setCreateError(false);
    setSourceUrlError(false);
    setActivatingResourceId(undefined);
    setActivationErrorId(undefined);
    setResourceFilter('all');
    setResourceQuery('');
  }, [visible]);

  const orderedBooks = useMemo(
    () => [...books].sort((left, right) => (right.last_opened_at ?? right.added_at).localeCompare(left.last_opened_at ?? left.added_at)),
    [books],
  );
  const currentBook = orderedBooks[0];
  const canCreate = title.trim().length > 0;
  const filteredResources = useMemo(() => {
    const query = resourceQuery.trim().toLocaleLowerCase();
    return resources
      .filter((resource) => resourceFilter === 'all' || resource.status === resourceFilter)
      .filter((resource) => !query || [resource.title, resource.creator, resource.summary, resource.body ?? '', ...resource.tags]
        .some((value) => value.toLocaleLowerCase().includes(query)))
      .sort((left, right) => right.updated_at.localeCompare(left.updated_at));
  }, [resourceFilter, resourceQuery, resources]);

  const createResource = async () => {
    if (!canCreate || creating) return;
    setCreating(true);
    setCreateError(false);
    setSourceUrlError(false);
    try {
      const body = content.trim();
      const normalizedSourceUrl = normalizeHttpUrl(sourceUrl);
      if (sourceUrl.trim() && !normalizedSourceUrl) {
        setSourceUrlError(true);
        return;
      }
      await onCreateResource({
        title: title.trim(),
        creator: author.trim(),
        summary: body.slice(0, 160),
        kind,
        status: kind === 'book' ? 'active' : 'inbox',
        source_url: normalizedSourceUrl || undefined,
        body: body || undefined,
        tags: normalizeTags(tags),
      });
      setAdding(false);
      setTitle('');
      setAuthor('');
      setContent('');
      setSourceUrl('');
      setTags('');
      setKind('book');
    } catch {
      setCreateError(true);
    } finally {
      setCreating(false);
    }
  };
  const startJourney = async (resource: KnowledgeResource) => {
    if (activatingResourceId) return;
    setActivatingResourceId(resource.id);
    setActivationErrorId(undefined);
    try {
      await onStartJourney(resource);
    } catch {
      setActivationErrorId(resource.id);
    } finally {
      setActivatingResourceId(undefined);
    }
  };

  return (
    <Modal animationType="slide" onRequestClose={onClose} visible={visible}>
      <SafeAreaView edges={['top', 'bottom', 'left', 'right']} style={styles.screen}>
        <KeyboardAvoidingView behavior={Platform.OS === 'ios' ? 'padding' : undefined} style={styles.screen}>
        <View style={styles.header}>
          <Pressable accessibilityLabel="关闭书架" hitSlop={10} onPress={onClose} style={styles.iconButton}>
            <X color={colors.ink} size={22} />
          </Pressable>
          <Text style={styles.headerTitle}>资源与知识库</Text>
          <Pressable accessibilityLabel="新建资源" hitSlop={10} onPress={() => setAdding((value) => !value)} style={styles.iconButton}>
            {adding ? <X color={colors.ink} size={21} /> : <Plus color={colors.evergreen} size={23} />}
          </Pressable>
        </View>

        <ScrollView contentContainerStyle={styles.content} keyboardShouldPersistTaps="handled" showsVerticalScrollIndicator={false}>
          {adding ? (
            <View style={styles.addForm}>
              <Text style={styles.formTitle}>新建资源</Text>
              <View style={styles.kindPicker}>{knowledgeKinds.map((option) => <Pressable accessibilityRole="button" key={option.kind} onPress={() => setKind(option.kind)} style={({ pressed }) => [styles.kindChoice, kind === option.kind && styles.kindChoiceActive, pressed && styles.pressed]}><Text style={[styles.kindChoiceText, kind === option.kind && styles.kindChoiceTextActive]}>{option.label}</Text></Pressable>)}</View>
              <TextInput accessibilityLabel="资源标题" onChangeText={setTitle} placeholder={kind === 'book' ? '书名' : '资源标题'} placeholderTextColor={colors.faint} style={styles.input} value={title} />
              <TextInput accessibilityLabel="作者或来源" onChangeText={setAuthor} placeholder="作者、机构或来源（可选）" placeholderTextColor={colors.faint} style={styles.input} value={author} />
              <TextInput accessibilityLabel="原始链接" autoCapitalize="none" autoCorrect={false} keyboardType="url" onChangeText={(value) => { setSourceUrl(value); setSourceUrlError(false); }} placeholder="原始链接（可选，仅支持 http 或 https）" placeholderTextColor={colors.faint} style={styles.input} value={sourceUrl} />
              <TextInput accessibilityLabel="资源标签" autoCapitalize="none" onChangeText={setTags} placeholder="标签（可选，使用逗号分隔）" placeholderTextColor={colors.faint} style={styles.input} value={tags} />
              <TextInput accessibilityLabel="资源正文或笔记" multiline onChangeText={setContent} placeholder={kind === 'book' ? '写下要阅读的段落或导读（可选）' : '摘录、要点或你想带走的问题（可选）'} placeholderTextColor={colors.faint} style={styles.contentInput} textAlignVertical="top" value={content} />
              {sourceUrlError ? <Text accessibilityLiveRegion="polite" style={styles.createError}>来源链接仅支持 http 或 https 地址</Text> : null}
              {createError ? <Text style={styles.createError}>保存失败，请检查网络后重试</Text> : null}
              <Pressable accessibilityRole="button" disabled={!canCreate || creating} onPress={createResource} style={({ pressed }) => [styles.createButton, (!canCreate || creating) && styles.disabled, pressed && canCreate && !creating && styles.pressed]}>
                {creating ? <ActivityIndicator color={colors.surface} size="small" /> : <BookOpenText color={colors.surface} size={18} />}
                <Text style={styles.createButtonText}>{creating ? '正在保存' : kind === 'book' ? '创建并阅读' : '收入收集箱'}</Text>
              </Pressable>
            </View>
          ) : null}

          <View style={styles.resourceBrowser}>
            <View style={styles.sectionHeader}>
              <View><Text style={styles.sectionTitle}>资源库</Text><Text style={styles.inboxHint}>从收藏、行动到完成，所有线索都留在这里</Text></View>
              <Text style={styles.sectionMeta}>{filteredResources.length} 条</Text>
            </View>
            <TextInput accessibilityLabel="搜索资源" onChangeText={setResourceQuery} placeholder="搜索标题、来源、笔记或标签" placeholderTextColor={colors.faint} style={styles.resourceSearch} value={resourceQuery} />
            <ScrollView contentContainerStyle={styles.resourceFilters} horizontal showsHorizontalScrollIndicator={false}>{resourceFilters.map((option) => <Pressable accessibilityRole="button" key={option.status} onPress={() => setResourceFilter(option.status)} style={({ pressed }) => [styles.resourceFilter, resourceFilter === option.status && styles.resourceFilterActive, pressed && styles.pressed]}><Text style={[styles.resourceFilterText, resourceFilter === option.status && styles.resourceFilterTextActive]}>{option.label}</Text></Pressable>)}</ScrollView>
            {filteredResources.length ? <View style={styles.inboxList}>{filteredResources.map((resource) => <ResourceCard activationError={activationErrorId === resource.id} activating={activatingResourceId === resource.id} activationBlocked={Boolean(activatingResourceId)} key={resource.id} onStartJourney={startJourney} onUpdateResource={onUpdateResource} resource={resource} />)}</View> : <Text style={styles.inboxEmpty}>{resourceQuery.trim() || resourceFilter !== 'all' ? '没有符合条件的资源。' : '在社区详情中点“收进知识库”，它会出现在这里。'}</Text>}
          </View>

          {currentBook ? (
            <View style={styles.continueSection}>
              <Text style={styles.eyebrow}>正在阅读</Text>
              <Pressable onPress={() => onOpenBook(currentBook)} style={({ pressed }) => [styles.currentBook, pressed && styles.pressed]}>
                <BookCover book={currentBook} size="large" />
                <View style={styles.currentCopy}>
                  <Text numberOfLines={2} style={styles.currentTitle}>{currentBook.title}</Text>
                  <Text numberOfLines={1} style={styles.currentAuthor}>{currentBook.author || '未署名'}</Text>
                  <Text numberOfLines={2} style={styles.currentSummary}>{currentBook.summary}</Text>
                  <View style={styles.progressLine}>
                    <View style={styles.track}><View style={[styles.fill, { width: `${currentBook.progress}%`, backgroundColor: currentBook.accent }]} /></View>
                    <Text style={styles.progressText}>{currentBook.progress}%</Text>
                  </View>
                </View>
                <ChevronRight color={colors.faint} size={19} />
              </Pressable>
            </View>
          ) : null}

          <View style={styles.sectionHeader}>
            <Text style={styles.sectionTitle}>阅读资源</Text>
            <Text style={styles.sectionMeta}>{books.length} 本</Text>
          </View>
          <View style={styles.bookList}>
            {orderedBooks.length ? orderedBooks.map((book) => (
              <Pressable key={book.id} onPress={() => onOpenBook(book)} style={({ pressed }) => [styles.bookRow, pressed && styles.pressed]}>
                <BookCover book={book} size="small" />
                <View style={styles.bookCopy}>
                  <Text numberOfLines={1} style={styles.bookTitle}>{book.title}</Text>
                  <Text numberOfLines={1} style={styles.bookAuthor}>{book.author || '未署名'}</Text>
                  <View style={styles.bookMeta}>
                    <Clock3 color={colors.faint} size={13} />
                    <Text style={styles.bookMetaText}>{formatReadingTime(book.reading_seconds)}</Text>
                    <Text style={styles.bookMetaText}>·</Text>
                    <Text style={styles.bookMetaText}>{bookmarkCount(book.id, bookmarks)} 个书签</Text>
                  </View>
                  <View style={styles.rowTrack}><View style={[styles.rowFill, { width: `${book.progress}%`, backgroundColor: book.accent }]} /></View>
                </View>
                <Text style={styles.rowProgress}>{book.progress}%</Text>
              </Pressable>
            )) : <Text style={styles.empty}>新建一本阅读文本，从想读的内容开始。</Text>}
          </View>
        </ScrollView>
        </KeyboardAvoidingView>
      </SafeAreaView>
    </Modal>
  );
}

function ResourceCard({
  resource,
  activating,
  activationBlocked,
  activationError,
  onStartJourney,
  onUpdateResource,
}: {
  resource: KnowledgeResource;
  activating: boolean;
  activationBlocked: boolean;
  activationError: boolean;
  onStartJourney: (resource: KnowledgeResource) => Promise<void>;
  onUpdateResource: (resourceId: string, input: UpdateKnowledgeResourceInput) => Promise<void>;
}) {
  const [updating, setUpdating] = useState(false);
  const [updateError, setUpdateError] = useState(false);
  const [sourceError, setSourceError] = useState(false);
  const sourceUrl = resource.source_url?.trim();
  const isCommunityReference = Boolean(resource.source_content_id) || sourceUrl?.startsWith('bookway://content/');

  const updateStatus = async (status: KnowledgeResourceStatus) => {
    if (updating) return;
    setUpdating(true);
    setUpdateError(false);
    try {
      await onUpdateResource(resource.id, { status });
    } catch {
      setUpdateError(true);
    } finally {
      setUpdating(false);
    }
  };
  const openSource = async () => {
    const url = normalizeHttpUrl(sourceUrl);
    if (!url) {
      setSourceError(true);
      return;
    }
    setSourceError(false);
    try {
      if (!await Linking.canOpenURL(url)) throw new Error('unsupported source URL');
      await Linking.openURL(url);
    } catch {
      setSourceError(true);
    }
  };
  const restoreLabel = resource.status === 'archived' ? '恢复到收集箱' : '重新收集';

  return <View style={styles.inboxCard}>
    <View style={styles.resourceHeading}><Text style={styles.resourceKind}>{knowledgeKindLabel(resource.kind)}</Text><Text style={styles.resourceStatus}>{knowledgeStatusLabel(resource.status)}</Text></View>
    <Text numberOfLines={2} style={styles.resourceTitle}>{resource.title}</Text>
    {resource.creator.trim() ? <Text numberOfLines={1} style={styles.resourceCreator}>{resource.creator}</Text> : null}
    {resource.summary.trim() ? <Text numberOfLines={2} style={styles.resourceSummary}>{resource.summary}</Text> : null}
    {resource.tags.length ? <Text numberOfLines={1} style={styles.resourceTags}>{resource.tags.slice(0, 3).map((tag) => `#${tag}`).join('  ')}</Text> : null}
    {sourceUrl && !isCommunityReference ? <Pressable accessibilityLabel={`打开${resource.title}的来源链接`} onPress={() => void openSource()} style={({ pressed }) => [styles.sourceLink, pressed && styles.pressed]}><ExternalLink color={colors.blue} size={13} /><Text numberOfLines={1} style={styles.resourceUrl}>{sourceUrl}</Text></Pressable> : null}
    {isCommunityReference ? <Text style={styles.communitySource}>来自社区内容；再次打开原内容时会重新核验可见性。</Text> : null}
    {resource.status === 'active' ? <Text style={styles.activeResourceMeta}>{resource.journey_id ? '已生成行动计划 · 在“我的路线”继续' : '正在使用'}</Text> : null}
    {activationError ? <Text accessibilityLiveRegion="polite" style={styles.activationError}>创建计划失败，请检查网络后重试</Text> : null}
    {updateError ? <Text accessibilityLiveRegion="polite" style={styles.activationError}>更新资源状态失败，请检查网络后重试</Text> : null}
    {sourceError ? <Text accessibilityLiveRegion="polite" style={styles.activationError}>来源链接无法安全打开</Text> : null}
    <View style={styles.resourceActions}>
      {resource.status === 'inbox' ? <Pressable accessibilityLabel={`将${resource.title}变成行动计划`} disabled={activationBlocked} onPress={() => void onStartJourney(resource)} style={({ pressed }) => [styles.activateButton, activationBlocked && styles.disabled, pressed && !activationBlocked && styles.pressed]}><BookOpenText color={colors.surface} size={16} /><Text style={styles.activateButtonText}>{activating ? '正在创建…' : '变成行动计划'}</Text></Pressable> : null}
      {resource.status === 'completed' || resource.status === 'archived' ? <Pressable accessibilityLabel={`${restoreLabel}${resource.title}`} disabled={updating} onPress={() => void updateStatus('inbox')} style={({ pressed }) => [styles.secondaryAction, updating && styles.disabled, pressed && !updating && styles.pressed]}><RotateCcw color={colors.evergreen} size={15} /><Text style={styles.secondaryActionText}>{updating ? '正在更新…' : restoreLabel}</Text></Pressable> : <Pressable accessibilityLabel={`将${resource.title}标记为完成`} disabled={updating} onPress={() => void updateStatus('completed')} style={({ pressed }) => [styles.secondaryAction, updating && styles.disabled, pressed && !updating && styles.pressed]}><Check color={colors.evergreen} size={15} /><Text style={styles.secondaryActionText}>{updating ? '正在更新…' : '标记完成'}</Text></Pressable>}
      {resource.status !== 'archived' ? <Pressable accessibilityLabel={`归档${resource.title}`} disabled={updating} onPress={() => void updateStatus('archived')} style={({ pressed }) => [styles.tertiaryAction, updating && styles.disabled, pressed && !updating && styles.pressed]}><Archive color={colors.muted} size={15} /><Text style={styles.tertiaryActionText}>归档</Text></Pressable> : null}
    </View>
  </View>;
}

function BookCover({ book, size }: { book: ReadingBook; size: 'large' | 'small' }) {
  const large = size === 'large';
  return (
    <View style={[styles.cover, large ? styles.largeCover : styles.smallCover, { backgroundColor: book.accent }]}>
      <Text numberOfLines={3} style={[styles.coverTitle, large && styles.largeCoverTitle]}>{book.title}</Text>
      <View style={styles.coverRule} />
      <Text numberOfLines={1} style={styles.coverAuthor}>{book.author || 'BOOKWAY'}</Text>
    </View>
  );
}

function bookmarkCount(bookId: string, bookmarks: ReadingBookmark[]) {
  return bookmarks.filter((bookmark) => bookmark.book_id === bookId).length;
}

function formatReadingTime(seconds: number) {
  const minutes = Math.round(seconds / 60);
  return minutes > 0 ? `已读 ${minutes} 分钟` : '尚未开始';
}

function knowledgeKindLabel(kind: KnowledgeResource['kind']) {
  return ({ article: '文章', book: '书籍', course: '课程', video: '视频', link: '链接', note: '笔记' })[kind];
}

function knowledgeStatusLabel(status: KnowledgeResourceStatus) {
  return ({ inbox: '收集箱', active: '正在实践', completed: '已完成', archived: '已归档' })[status];
}

function normalizeTags(value: string) {
  const seen = new Set<string>();
  return value.split(/[，,\n]/)
    .map((tag) => tag.trim().replace(/^#/, ''))
    .filter((tag) => tag && !seen.has(tag.toLocaleLowerCase()) && Boolean(seen.add(tag.toLocaleLowerCase())))
    .slice(0, 12);
}

function normalizeHttpUrl(value?: string) {
  const candidate = value?.trim();
  if (!candidate) return undefined;
  try {
    const url = new URL(candidate);
    return url.protocol === 'http:' || url.protocol === 'https:' ? url.toString() : undefined;
  } catch {
    return undefined;
  }
}

const knowledgeKinds: Array<{ kind: KnowledgeResourceKind; label: string }> = [
  { kind: 'book', label: '书籍' },
  { kind: 'article', label: '文章' },
  { kind: 'course', label: '课程' },
  { kind: 'video', label: '视频' },
  { kind: 'link', label: '链接' },
  { kind: 'note', label: '笔记' },
];

const resourceFilters: Array<{ status: KnowledgeResourceStatus | 'all'; label: string }> = [
  { status: 'all', label: '全部' },
  { status: 'inbox', label: '收集箱' },
  { status: 'active', label: '实践中' },
  { status: 'completed', label: '已完成' },
  { status: 'archived', label: '已归档' },
];

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { height: 64, paddingHorizontal: 16, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', backgroundColor: colors.surface, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  iconButton: { width: 42, height: 42, alignItems: 'center', justifyContent: 'center' },
  headerTitle: { flex: 1, color: colors.ink, fontSize: 16, fontWeight: '700', textAlign: 'center', letterSpacing: 0 },
  content: { paddingBottom: 38 },
  addForm: { gap: 10, padding: 20, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth, backgroundColor: colors.surface },
  formTitle: { color: colors.ink, fontSize: 16, fontWeight: '700', marginBottom: 2, letterSpacing: 0 },
  kindPicker: { flexDirection: 'row', flexWrap: 'wrap', gap: 7, marginBottom: 2 },
  kindChoice: { minHeight: 30, paddingHorizontal: 10, alignItems: 'center', justifyContent: 'center', borderWidth: 1, borderColor: colors.line, borderRadius: 15, backgroundColor: colors.background },
  kindChoiceActive: { borderColor: colors.evergreen, backgroundColor: colors.evergreenSoft },
  kindChoiceText: { color: colors.muted, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  kindChoiceTextActive: { color: colors.evergreen },
  input: { height: 44, paddingHorizontal: 12, borderWidth: 1, borderColor: colors.line, borderRadius: 6, color: colors.ink, backgroundColor: colors.background, fontSize: 14, letterSpacing: 0 },
  contentInput: { minHeight: 112, padding: 12, borderWidth: 1, borderColor: colors.line, borderRadius: 6, color: colors.ink, backgroundColor: colors.background, fontSize: 14, lineHeight: 21, letterSpacing: 0 },
  createButton: { height: 46, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 8, borderRadius: 6, backgroundColor: colors.evergreen },
  createButtonText: { color: colors.surface, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  createError: { color: colors.coral, fontSize: 12, lineHeight: 18, letterSpacing: 0 },
  disabled: { opacity: 0.35 },
  inboxSection: { borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth, backgroundColor: colors.surface },
  activeSection: { borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth, backgroundColor: colors.surface },
  resourceBrowser: { paddingBottom: 20, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth, backgroundColor: colors.surface },
  inboxHint: { color: colors.muted, fontSize: 11, lineHeight: 17, marginTop: 3, letterSpacing: 0 },
  resourceSearch: { height: 40, marginHorizontal: 20, paddingHorizontal: 11, borderWidth: 1, borderColor: colors.line, borderRadius: 6, color: colors.ink, backgroundColor: colors.background, fontSize: 13, letterSpacing: 0 },
  resourceFilters: { gap: 7, paddingHorizontal: 20, paddingVertical: 11 },
  resourceFilter: { minHeight: 30, paddingHorizontal: 11, alignItems: 'center', justifyContent: 'center', borderWidth: 1, borderColor: colors.line, borderRadius: 15, backgroundColor: colors.background },
  resourceFilterActive: { borderColor: colors.evergreen, backgroundColor: colors.evergreenSoft },
  resourceFilterText: { color: colors.muted, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  resourceFilterTextActive: { color: colors.evergreen },
  inboxList: { paddingHorizontal: 20, paddingBottom: 20, gap: 10 },
  inboxCard: { padding: 14, borderWidth: 1, borderColor: colors.line, borderRadius: 7, backgroundColor: colors.background },
  resourceHeading: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 8 },
  resourceKind: { color: colors.evergreen, fontSize: 10, fontWeight: '700', letterSpacing: 0 },
  resourceStatus: { color: colors.faint, fontSize: 10, fontWeight: '700', letterSpacing: 0 },
  resourceTitle: { color: colors.ink, fontSize: 15, lineHeight: 21, fontWeight: '700', marginTop: 5, letterSpacing: 0 },
  resourceCreator: { color: colors.muted, fontSize: 11, marginTop: 4, letterSpacing: 0 },
  resourceSummary: { color: colors.muted, fontSize: 12, lineHeight: 19, marginTop: 7, letterSpacing: 0 },
  resourceTags: { color: colors.blue, fontSize: 11, marginTop: 8, letterSpacing: 0 },
  sourceLink: { maxWidth: '100%', marginTop: 8, flexDirection: 'row', alignItems: 'center', gap: 5 },
  resourceUrl: { flex: 1, minWidth: 0, color: colors.blue, fontSize: 11, letterSpacing: 0 },
  communitySource: { color: colors.faint, fontSize: 11, lineHeight: 17, marginTop: 8, letterSpacing: 0 },
  activeResourceMeta: { color: colors.evergreen, fontSize: 11, fontWeight: '700', marginTop: 10, letterSpacing: 0 },
  resourceActions: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, marginTop: 12 },
  activateButton: { height: 38, paddingHorizontal: 12, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 7, borderRadius: 5, backgroundColor: colors.evergreen },
  activateButtonText: { color: colors.surface, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  secondaryAction: { height: 38, paddingHorizontal: 11, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 6, borderRadius: 5, borderWidth: 1, borderColor: colors.evergreen, backgroundColor: colors.evergreenSoft },
  secondaryActionText: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  tertiaryAction: { height: 38, paddingHorizontal: 11, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 6, borderRadius: 5, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  tertiaryActionText: { color: colors.muted, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  activationError: { color: colors.coral, fontSize: 11, lineHeight: 17, marginTop: 8, letterSpacing: 0 },
  inboxEmpty: { paddingHorizontal: 20, paddingBottom: 20, color: colors.faint, fontSize: 12, lineHeight: 19, letterSpacing: 0 },
  continueSection: { padding: 20, backgroundColor: colors.surface },
  eyebrow: { color: colors.evergreen, fontSize: 11, fontWeight: '700', marginBottom: 11, letterSpacing: 0 },
  currentBook: { minHeight: 156, flexDirection: 'row', alignItems: 'center', gap: 13 },
  currentCopy: { flex: 1, minWidth: 0, alignSelf: 'stretch', justifyContent: 'center' },
  currentTitle: { color: colors.ink, fontSize: 18, lineHeight: 25, fontWeight: '800', letterSpacing: 0 },
  currentAuthor: { color: colors.muted, fontSize: 12, marginTop: 4, letterSpacing: 0 },
  currentSummary: { color: colors.muted, fontSize: 12, lineHeight: 18, marginTop: 9, letterSpacing: 0 },
  progressLine: { flexDirection: 'row', alignItems: 'center', gap: 8, marginTop: 12 },
  track: { flex: 1, height: 5, overflow: 'hidden', borderRadius: 3, backgroundColor: colors.line },
  fill: { height: 5, borderRadius: 3 },
  progressText: { color: colors.muted, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  sectionHeader: { height: 64, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  sectionTitle: { color: colors.ink, fontSize: 16, fontWeight: '700', letterSpacing: 0 },
  sectionMeta: { color: colors.faint, fontSize: 11, letterSpacing: 0 },
  bookList: { borderTopColor: colors.line, borderTopWidth: StyleSheet.hairlineWidth, backgroundColor: colors.surface },
  bookRow: { minHeight: 102, paddingHorizontal: 20, paddingVertical: 12, flexDirection: 'row', alignItems: 'center', gap: 12, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  bookCopy: { flex: 1, minWidth: 0, justifyContent: 'center' },
  bookTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  bookAuthor: { color: colors.muted, fontSize: 11, marginTop: 3, letterSpacing: 0 },
  bookMeta: { flexDirection: 'row', alignItems: 'center', gap: 4, marginTop: 8 },
  bookMetaText: { color: colors.faint, fontSize: 10, letterSpacing: 0 },
  rowTrack: { height: 4, overflow: 'hidden', marginTop: 8, borderRadius: 2, backgroundColor: colors.line },
  rowFill: { height: 4, borderRadius: 2 },
  rowProgress: { color: colors.muted, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  cover: { overflow: 'hidden', justifyContent: 'flex-end', padding: 9, borderRadius: 4 },
  largeCover: { width: 88, height: 132 },
  smallCover: { width: 54, height: 78 },
  coverTitle: { color: colors.surface, fontSize: 11, lineHeight: 15, fontWeight: '800', letterSpacing: 0 },
  largeCoverTitle: { fontSize: 14, lineHeight: 19 },
  coverRule: { width: 22, height: 2, marginVertical: 7, backgroundColor: 'rgba(255,255,255,0.72)' },
  coverAuthor: { color: 'rgba(255,255,255,0.8)', fontSize: 8, letterSpacing: 0 },
  empty: { padding: 32, color: colors.faint, fontSize: 13, lineHeight: 20, textAlign: 'center', letterSpacing: 0 },
  pressed: { opacity: 0.62 },
});
