import { Bookmark, ChevronLeft, ChevronRight, Check, List, Minus, Plus, Settings2, X } from 'lucide-react-native';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Modal, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { colors } from '../theme';
import { Action, ReaderSettings, ReadingBook, ReadingBookmark } from '../types';

type Props = {
  book?: ReadingBook;
  bookmarks: ReadingBookmark[];
  linkedAction?: Action;
  settings: ReaderSettings;
  visible: boolean;
  onClose: () => void;
  onSaveProgress: (bookId: string, updates: Partial<Pick<ReadingBook, 'progress' | 'current_chapter' | 'last_opened_at' | 'reading_seconds'>>) => void;
  onToggleBookmark: (bookId: string, chapterId: string) => void;
  onUpdateSettings: (updates: Partial<ReaderSettings>) => void;
  onCompleteAction?: (actionId: string) => void;
};

export function ReaderModal({ book, bookmarks, linkedAction, settings, visible, onClose, onSaveProgress, onToggleBookmark, onUpdateSettings, onCompleteAction }: Props) {
  const [chapterIndex, setChapterIndex] = useState(0);
  const [tocVisible, setTocVisible] = useState(false);
  const [settingsVisible, setSettingsVisible] = useState(false);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const openedAtRef = useRef(0);

  useEffect(() => {
    if (!visible || !book) return;
    setChapterIndex(Math.max(0, Math.min(book.current_chapter, book.chapters.length - 1)));
    setTocVisible(false);
    setSettingsVisible(false);
    setElapsedSeconds(0);
    openedAtRef.current = Date.now();
  }, [book?.id, visible]);

  useEffect(() => {
    if (!visible) return;
    const timer = setInterval(() => setElapsedSeconds((seconds) => seconds + 1), 1000);
    return () => clearInterval(timer);
  }, [visible]);

  if (!book) return null;
  const chapter = book.chapters[chapterIndex];
  const palette = settings.theme === 'night' ? nightPalette : lightPalette;
  const bookmarked = bookmarks.some((item) => item.book_id === book.id && item.chapter_id === chapter.id);
  const progress = Math.max(book.progress, Math.round(((chapterIndex + 1) / book.chapters.length) * 100));
  const canGoBack = chapterIndex > 0;
  const canGoNext = chapterIndex < book.chapters.length - 1;

  const saveProgress = (nextIndex: number, includeDuration = false) => {
    const nextProgress = Math.max(book.progress, Math.round(((nextIndex + 1) / book.chapters.length) * 100));
    onSaveProgress(book.id, {
      current_chapter: nextIndex,
      progress: nextProgress,
      last_opened_at: new Date().toISOString(),
      ...(includeDuration ? { reading_seconds: book.reading_seconds + elapsedSeconds } : {}),
    });
  };

  const chooseChapter = (index: number) => {
    if (index === chapterIndex) {
      setTocVisible(false);
      return;
    }
    setChapterIndex(index);
    saveProgress(index);
    setTocVisible(false);
  };

  const close = () => {
    saveProgress(chapterIndex, true);
    onClose();
  };

  const completeAction = () => {
    if (!linkedAction || linkedAction.state === 'completed') return;
    saveProgress(chapterIndex, true);
    onCompleteAction?.(linkedAction.id);
  };

  return (
    <Modal animationType="slide" onRequestClose={close} visible={visible}>
      <SafeAreaView edges={['top', 'bottom', 'left', 'right']} style={[styles.screen, { backgroundColor: palette.background }]}>
        <View style={styles.screen}>
        <View style={[styles.toolbar, { borderBottomColor: palette.line }]}>
          <Pressable accessibilityLabel="关闭阅读器" hitSlop={10} onPress={close} style={styles.iconButton}>
            <X color={palette.foreground} size={22} />
          </Pressable>
          <View style={styles.titleBox}>
            <Text numberOfLines={1} style={[styles.bookTitle, { color: palette.foreground }]}>{book.title}</Text>
            <Text numberOfLines={1} style={[styles.chapterTitle, { color: palette.muted }]}>{chapter.title}</Text>
          </View>
          <Pressable accessibilityLabel="目录" hitSlop={10} onPress={() => setTocVisible(true)} style={styles.iconButton}>
            <List color={palette.foreground} size={21} />
          </Pressable>
          <Pressable accessibilityLabel="阅读设置" hitSlop={10} onPress={() => setSettingsVisible(true)} style={styles.iconButton}>
            <Settings2 color={palette.foreground} size={21} />
          </Pressable>
        </View>

        <ScrollView contentContainerStyle={styles.readingContent} showsVerticalScrollIndicator={false}>
          <Text style={[styles.readerEyebrow, { color: palette.accent }]}>第 {chapterIndex + 1} 章</Text>
          <Text style={[styles.readerHeading, { color: palette.foreground }]}>{chapter.title}</Text>
          {chapter.body.map((paragraph, index) => (
            <Text key={`${chapter.id}-${index}`} style={[styles.paragraph, { color: palette.foreground, fontSize: settings.font_size, lineHeight: settings.font_size * settings.line_height }]}>{paragraph}</Text>
          ))}
        </ScrollView>

        <View style={[styles.footer, { borderTopColor: palette.line }]}>
          <View style={styles.statusLine}>
            <Text style={[styles.statusText, { color: palette.muted }]}>{formatDuration(elapsedSeconds)}</Text>
            <Text style={[styles.statusText, { color: palette.muted }]}>{progress}% · {chapterIndex + 1}/{book.chapters.length}</Text>
          </View>
          <View style={[styles.progressTrack, { backgroundColor: palette.line }]}><View style={[styles.progressFill, { width: `${progress}%`, backgroundColor: palette.accent }]} /></View>
          <View style={styles.actions}>
            <Pressable accessibilityLabel={bookmarked ? '移除书签' : '添加书签'} onPress={() => onToggleBookmark(book.id, chapter.id)} style={[styles.bookmarkButton, { borderColor: palette.line }]}>
              <Bookmark color={bookmarked ? palette.accent : palette.foreground} fill={bookmarked ? palette.accent : 'transparent'} size={19} />
            </Pressable>
            <Pressable accessibilityLabel="上一章" disabled={!canGoBack} onPress={() => { const nextIndex = chapterIndex - 1; setChapterIndex(nextIndex); saveProgress(nextIndex); }} style={[styles.pageButton, { borderColor: palette.line }, !canGoBack && styles.disabled]}>
              <ChevronLeft color={palette.foreground} size={19} />
            </Pressable>
            <Pressable accessibilityLabel="下一章" disabled={!canGoNext} onPress={() => { const nextIndex = chapterIndex + 1; setChapterIndex(nextIndex); saveProgress(nextIndex); }} style={[styles.pageButton, { borderColor: palette.line }, !canGoNext && styles.disabled]}>
              <ChevronRight color={palette.foreground} size={19} />
            </Pressable>
            {linkedAction && linkedAction.state !== 'completed' ? (
              <Pressable onPress={completeAction} style={[styles.completeButton, { backgroundColor: palette.accent }]}>
                <Check color={colors.surface} size={18} strokeWidth={3} />
                <Text style={styles.completeText}>完成本次阅读</Text>
              </Pressable>
            ) : null}
          </View>
        </View>

        <TableOfContents activeIndex={chapterIndex} book={book} palette={palette} visible={tocVisible} onClose={() => setTocVisible(false)} onSelect={chooseChapter} />
        <ReaderSettingsSheet settings={settings} palette={palette} visible={settingsVisible} onClose={() => setSettingsVisible(false)} onUpdate={onUpdateSettings} />
        </View>
      </SafeAreaView>
    </Modal>
  );
}

function TableOfContents({ book, activeIndex, palette, visible, onClose, onSelect }: { book: ReadingBook; activeIndex: number; palette: Palette; visible: boolean; onClose: () => void; onSelect: (index: number) => void }) {
  return (
    <Modal animationType="fade" onRequestClose={onClose} transparent visible={visible}>
      <View style={styles.overlay}>
        <Pressable accessibilityLabel="关闭目录" onPress={onClose} style={styles.overlayDismiss} />
        <View style={[styles.tocSheet, { backgroundColor: palette.background }]}>
          <View style={[styles.sheetHeader, { borderBottomColor: palette.line }]}>
            <Text style={[styles.sheetTitle, { color: palette.foreground }]}>目录</Text>
            <Pressable accessibilityLabel="关闭目录" onPress={onClose} style={styles.iconButton}><X color={palette.foreground} size={21} /></Pressable>
          </View>
          <ScrollView showsVerticalScrollIndicator={false}>
            {book.chapters.map((chapter, index) => {
              const selected = index === activeIndex;
              return (
                <Pressable key={chapter.id} onPress={() => onSelect(index)} style={[styles.tocRow, { borderBottomColor: palette.line }, selected && { backgroundColor: palette.selected }]}>
                  <Text style={[styles.tocIndex, { color: selected ? palette.accent : palette.muted }]}>{String(index + 1).padStart(2, '0')}</Text>
                  <Text style={[styles.tocText, { color: palette.foreground }, selected && { color: palette.accent }]}>{chapter.title}</Text>
                </Pressable>
              );
            })}
          </ScrollView>
        </View>
      </View>
    </Modal>
  );
}

function ReaderSettingsSheet({ settings, palette, visible, onClose, onUpdate }: { settings: ReaderSettings; palette: Palette; visible: boolean; onClose: () => void; onUpdate: (updates: Partial<ReaderSettings>) => void }) {
  const fontMinusDisabled = settings.font_size <= 14;
  const fontPlusDisabled = settings.font_size >= 28;
  const lineMinusDisabled = settings.line_height <= 1.4;
  const linePlusDisabled = settings.line_height >= 2.2;
  return (
    <Modal animationType="slide" onRequestClose={onClose} transparent visible={visible}>
      <View style={styles.settingsOverlay}>
        <View style={[styles.settingsSheet, { backgroundColor: palette.background }]}>
          <View style={[styles.sheetHeader, { borderBottomColor: palette.line }]}>
            <Text style={[styles.sheetTitle, { color: palette.foreground }]}>阅读设置</Text>
            <Pressable accessibilityLabel="关闭阅读设置" onPress={onClose} style={styles.iconButton}><X color={palette.foreground} size={21} /></Pressable>
          </View>
          <View style={styles.settingRow}>
            <Text style={[styles.settingLabel, { color: palette.foreground }]}>字号</Text>
            <Stepper decreaseDisabled={fontMinusDisabled} increaseDisabled={fontPlusDisabled} label={`${settings.font_size}`} onDecrease={() => onUpdate({ font_size: Math.max(14, settings.font_size - 1) })} onIncrease={() => onUpdate({ font_size: Math.min(28, settings.font_size + 1) })} palette={palette} />
          </View>
          <View style={styles.settingRow}>
            <Text style={[styles.settingLabel, { color: palette.foreground }]}>行距</Text>
            <Stepper decreaseDisabled={lineMinusDisabled} increaseDisabled={linePlusDisabled} label={settings.line_height.toFixed(1)} onDecrease={() => onUpdate({ line_height: Math.max(1.4, Number((settings.line_height - 0.1).toFixed(1))) })} onIncrease={() => onUpdate({ line_height: Math.min(2.2, Number((settings.line_height + 0.1).toFixed(1))) })} palette={palette} />
          </View>
          <View style={[styles.settingRow, styles.themeRow]}>
            <Text style={[styles.settingLabel, { color: palette.foreground }]}>主题</Text>
            <View style={styles.swatches}>
              <Pressable accessibilityLabel="浅色主题" accessibilityRole="radio" accessibilityState={{ checked: settings.theme === 'light' }} onPress={() => onUpdate({ theme: 'light' })} style={[styles.swatch, styles.lightSwatch, settings.theme === 'light' && styles.swatchSelected]} />
              <Pressable accessibilityLabel="夜间主题" accessibilityRole="radio" accessibilityState={{ checked: settings.theme === 'night' }} onPress={() => onUpdate({ theme: 'night' })} style={[styles.swatch, styles.nightSwatch, settings.theme === 'night' && styles.swatchSelected]} />
            </View>
          </View>
        </View>
      </View>
    </Modal>
  );
}

function Stepper({ label, palette, decreaseDisabled, increaseDisabled, onDecrease, onIncrease }: { label: string; palette: Palette; decreaseDisabled: boolean; increaseDisabled: boolean; onDecrease: () => void; onIncrease: () => void }) {
  return (
    <View style={styles.stepper}>
      <Pressable accessibilityLabel="减小" disabled={decreaseDisabled} onPress={onDecrease} style={[styles.stepperButton, { borderColor: palette.line }, decreaseDisabled && styles.disabled]}><Minus color={palette.foreground} size={18} /></Pressable>
      <Text style={[styles.stepperValue, { color: palette.foreground }]}>{label}</Text>
      <Pressable accessibilityLabel="增大" disabled={increaseDisabled} onPress={onIncrease} style={[styles.stepperButton, { borderColor: palette.line }, increaseDisabled && styles.disabled]}><Plus color={palette.foreground} size={18} /></Pressable>
    </View>
  );
}

type Palette = { background: string; foreground: string; muted: string; line: string; accent: string; selected: string };

const lightPalette: Palette = { background: '#F8FAF7', foreground: colors.ink, muted: colors.muted, line: colors.line, accent: colors.evergreen, selected: colors.evergreenSoft };
const nightPalette: Palette = { background: '#1D2521', foreground: '#EDF1ED', muted: '#B6C0B9', line: '#39443E', accent: '#8DB7A0', selected: '#29362F' };

function formatDuration(seconds: number) {
  const minutes = Math.floor(seconds / 60);
  const remainder = seconds % 60;
  return `${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`;
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  toolbar: { minHeight: 64, paddingHorizontal: 12, flexDirection: 'row', alignItems: 'center', borderBottomWidth: StyleSheet.hairlineWidth },
  iconButton: { width: 42, height: 42, alignItems: 'center', justifyContent: 'center' },
  titleBox: { flex: 1, minWidth: 0, paddingHorizontal: 4 },
  bookTitle: { fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  chapterTitle: { fontSize: 11, marginTop: 2, letterSpacing: 0 },
  readingContent: { flexGrow: 1, width: '100%', maxWidth: 760, alignSelf: 'center', paddingHorizontal: 24, paddingTop: 45, paddingBottom: 54 },
  readerEyebrow: { fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  readerHeading: { fontSize: 28, lineHeight: 38, fontWeight: '800', marginTop: 10, marginBottom: 30, letterSpacing: 0 },
  paragraph: { marginBottom: 20, letterSpacing: 0 },
  footer: { paddingHorizontal: 16, paddingTop: 12, paddingBottom: 14, borderTopWidth: StyleSheet.hairlineWidth },
  statusLine: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  statusText: { fontSize: 11, letterSpacing: 0 },
  progressTrack: { height: 4, overflow: 'hidden', marginTop: 8, borderRadius: 2 },
  progressFill: { height: 4, borderRadius: 2 },
  actions: { minHeight: 44, marginTop: 13, flexDirection: 'row', alignItems: 'center', justifyContent: 'flex-end', gap: 8 },
  bookmarkButton: { width: 44, height: 44, alignItems: 'center', justifyContent: 'center', borderWidth: 1, borderRadius: 6 },
  pageButton: { width: 44, height: 44, alignItems: 'center', justifyContent: 'center', borderWidth: 1, borderRadius: 6 },
  completeButton: { height: 44, paddingHorizontal: 13, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 7, borderRadius: 6 },
  completeText: { color: colors.surface, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  overlay: { flex: 1, flexDirection: 'row', backgroundColor: 'rgba(17, 23, 19, 0.34)' },
  overlayDismiss: { flex: 1 },
  tocSheet: { width: '82%', maxWidth: 360, height: '100%' },
  sheetHeader: { height: 64, paddingLeft: 20, paddingRight: 10, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', borderBottomWidth: StyleSheet.hairlineWidth },
  sheetTitle: { fontSize: 17, fontWeight: '800', letterSpacing: 0 },
  tocRow: { minHeight: 58, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', gap: 13, borderBottomWidth: StyleSheet.hairlineWidth },
  tocIndex: { width: 23, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  tocText: { flex: 1, fontSize: 14, lineHeight: 20, fontWeight: '600', letterSpacing: 0 },
  settingsOverlay: { flex: 1, justifyContent: 'flex-end', backgroundColor: 'rgba(17, 23, 19, 0.34)' },
  settingsSheet: { borderTopLeftRadius: 8, borderTopRightRadius: 8 },
  settingRow: { minHeight: 68, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  themeRow: { paddingBottom: 14 },
  settingLabel: { fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  stepper: { flexDirection: 'row', alignItems: 'center', gap: 10 },
  stepperButton: { width: 36, height: 36, alignItems: 'center', justifyContent: 'center', borderWidth: 1, borderRadius: 6 },
  stepperValue: { width: 35, textAlign: 'center', fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  swatches: { flexDirection: 'row', gap: 12 },
  swatch: { width: 32, height: 32, borderRadius: 16, borderWidth: 2, borderColor: 'transparent' },
  lightSwatch: { backgroundColor: '#F8FAF7', borderColor: colors.line },
  nightSwatch: { backgroundColor: '#1D2521' },
  swatchSelected: { borderColor: colors.evergreen },
  disabled: { opacity: 0.32 },
});
