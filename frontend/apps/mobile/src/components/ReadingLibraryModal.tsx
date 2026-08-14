import { BookOpenText, ChevronRight, Clock3, Plus, X } from 'lucide-react-native';
import { useEffect, useMemo, useState } from 'react';
import {
  KeyboardAvoidingView,
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
import { CreateReadingBookInput, ReadingBook, ReadingBookmark } from '../types';

type Props = {
  books: ReadingBook[];
  bookmarks: ReadingBookmark[];
  visible: boolean;
  onClose: () => void;
  onOpenBook: (book: ReadingBook) => void;
  onCreateBook: (input: CreateReadingBookInput) => void;
};

export function ReadingLibraryModal({ books, bookmarks, visible, onClose, onOpenBook, onCreateBook }: Props) {
  const [adding, setAdding] = useState(false);
  const [title, setTitle] = useState('');
  const [author, setAuthor] = useState('');
  const [content, setContent] = useState('');

  useEffect(() => {
    if (!visible) return;
    setAdding(false);
    setTitle('');
    setAuthor('');
    setContent('');
  }, [visible]);

  const orderedBooks = useMemo(
    () => [...books].sort((left, right) => (right.last_opened_at ?? right.added_at).localeCompare(left.last_opened_at ?? left.added_at)),
    [books],
  );
  const currentBook = orderedBooks[0];
  const canCreate = title.trim().length > 0;

  const createBook = () => {
    if (!canCreate) return;
    onCreateBook({ title: title.trim(), author: author.trim(), content: content.trim() || undefined });
    setAdding(false);
    setTitle('');
    setAuthor('');
    setContent('');
  };

  return (
    <Modal animationType="slide" onRequestClose={onClose} visible={visible}>
      <SafeAreaView edges={['top', 'bottom', 'left', 'right']} style={styles.screen}>
        <KeyboardAvoidingView behavior={Platform.OS === 'ios' ? 'padding' : undefined} style={styles.screen}>
        <View style={styles.header}>
          <Pressable accessibilityLabel="关闭书架" hitSlop={10} onPress={onClose} style={styles.iconButton}>
            <X color={colors.ink} size={22} />
          </Pressable>
          <Text style={styles.headerTitle}>我的书架</Text>
          <Pressable accessibilityLabel="新建阅读文本" hitSlop={10} onPress={() => setAdding((value) => !value)} style={styles.iconButton}>
            {adding ? <X color={colors.ink} size={21} /> : <Plus color={colors.evergreen} size={23} />}
          </Pressable>
        </View>

        <ScrollView contentContainerStyle={styles.content} keyboardShouldPersistTaps="handled" showsVerticalScrollIndicator={false}>
          {adding ? (
            <View style={styles.addForm}>
              <Text style={styles.formTitle}>新建阅读文本</Text>
              <TextInput accessibilityLabel="书名" onChangeText={setTitle} placeholder="书名" placeholderTextColor={colors.faint} style={styles.input} value={title} />
              <TextInput accessibilityLabel="作者" onChangeText={setAuthor} placeholder="作者或来源（可选）" placeholderTextColor={colors.faint} style={styles.input} value={author} />
              <TextInput accessibilityLabel="阅读正文" multiline onChangeText={setContent} placeholder="写下要阅读的段落或导读（可选）" placeholderTextColor={colors.faint} style={styles.contentInput} textAlignVertical="top" value={content} />
              <Pressable accessibilityRole="button" disabled={!canCreate} onPress={createBook} style={({ pressed }) => [styles.createButton, !canCreate && styles.disabled, pressed && canCreate && styles.pressed]}>
                <BookOpenText color={colors.surface} size={18} />
                <Text style={styles.createButtonText}>创建并阅读</Text>
              </Pressable>
            </View>
          ) : null}

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
            <Text style={styles.sectionTitle}>书架</Text>
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

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { height: 64, paddingHorizontal: 16, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', backgroundColor: colors.surface, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  iconButton: { width: 42, height: 42, alignItems: 'center', justifyContent: 'center' },
  headerTitle: { flex: 1, color: colors.ink, fontSize: 16, fontWeight: '700', textAlign: 'center', letterSpacing: 0 },
  content: { paddingBottom: 38 },
  addForm: { gap: 10, padding: 20, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth, backgroundColor: colors.surface },
  formTitle: { color: colors.ink, fontSize: 16, fontWeight: '700', marginBottom: 2, letterSpacing: 0 },
  input: { height: 44, paddingHorizontal: 12, borderWidth: 1, borderColor: colors.line, borderRadius: 6, color: colors.ink, backgroundColor: colors.background, fontSize: 14, letterSpacing: 0 },
  contentInput: { minHeight: 112, padding: 12, borderWidth: 1, borderColor: colors.line, borderRadius: 6, color: colors.ink, backgroundColor: colors.background, fontSize: 14, lineHeight: 21, letterSpacing: 0 },
  createButton: { height: 46, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 8, borderRadius: 6, backgroundColor: colors.evergreen },
  createButtonText: { color: colors.surface, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  disabled: { opacity: 0.35 },
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
