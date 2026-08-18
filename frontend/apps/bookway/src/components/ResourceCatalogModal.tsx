import { BookOpen, ExternalLink, Search, X } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import { ActivityIndicator, Linking, Modal, Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';

import { getPublicResources } from '../api/client';
import { colors } from '../theme';
import { PublicResource } from '../types';

type Props = { visible: boolean; onClose: () => void };
type Kind = PublicResource['kind'];
const kinds: Array<{ value?: Kind; label: string }> = [{ label: '全部' }, { value: 'book', label: '书籍' }, { value: 'course', label: '课程' }, { value: 'tool', label: '工具' }, { value: 'article', label: '文章' }, { value: 'podcast', label: '播客' }];

export function ResourceCatalogModal({ visible, onClose }: Props) {
  const [query, setQuery] = useState('');
  const [kind, setKind] = useState<Kind>();
  const [resources, setResources] = useState<PublicResource[]>([]);
  const [selected, setSelected] = useState<PublicResource>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);

  useEffect(() => {
    if (!visible) return;
    setSelected(undefined);
    setQuery('');
    setKind(undefined);
    void load();
  }, [visible]);

  const load = async (nextQuery = query, nextKind = kind) => {
    setLoading(true);
    setError(false);
    try {
      const page = await getPublicResources({ query: nextQuery.trim() || undefined, kind: nextKind, limit: 30 });
      setResources(page.items);
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  };

  const selectKind = (nextKind?: Kind) => {
    setKind(nextKind);
    void load(query, nextKind);
  };

  return <Modal animationType="slide" onRequestClose={onClose} visible={visible}><View style={styles.screen}>
    <View style={styles.header}><Pressable accessibilityLabel="关闭公共资源目录" hitSlop={10} onPress={onClose} style={styles.icon}><X color={colors.ink} size={21} /></Pressable><Text style={styles.title}>公共资源</Text><View style={styles.icon} /></View>
    <View style={styles.search}><Search color={colors.faint} size={17} /><TextInput accessibilityLabel="搜索公共资源" onChangeText={setQuery} onSubmitEditing={() => void load()} placeholder="搜索书籍、课程、工具…" placeholderTextColor={colors.faint} returnKeyType="search" style={styles.searchInput} value={query} /></View>
    <ScrollView horizontal contentContainerStyle={styles.kinds} showsHorizontalScrollIndicator={false}>{kinds.map((item) => <Pressable key={item.label} onPress={() => selectKind(item.value)} style={[styles.kind, kind === item.value && styles.kindActive]}><Text style={[styles.kindText, kind === item.value && styles.kindActiveText]}>{item.label}</Text></Pressable>)}</ScrollView>
    {loading ? <View style={styles.loading}><ActivityIndicator color={colors.evergreen} size="small" /></View> : error ? <View style={styles.empty}><Text style={styles.emptyTitle}>资源暂时不可用</Text><Text style={styles.muted}>请稍后重试。</Text></View> : <ScrollView contentContainerStyle={styles.list} showsVerticalScrollIndicator={false}>{resources.length === 0 ? <View style={styles.empty}><BookOpen color={colors.evergreen} size={25} /><Text style={styles.emptyTitle}>没有匹配的资源</Text><Text style={styles.muted}>换一个主题或关键词试试。</Text></View> : resources.map((resource) => <Pressable key={resource.id} onPress={() => setSelected(resource)} style={({ pressed }) => [styles.card, pressed && styles.pressed]}><View style={styles.cardTop}><Text style={styles.resourceKind}>{kindLabel(resource.kind)}</Text><Text style={styles.provider}>{resource.provider}</Text></View><Text style={styles.resourceTitle}>{resource.title}</Text><Text numberOfLines={2} style={styles.summary}>{resource.summary}</Text><View style={styles.topics}>{resource.topics.slice(0, 3).map((topic) => <Text key={topic} style={styles.topic}>#{topic}</Text>)}</View></Pressable>)}</ScrollView>}
    {selected ? <View style={styles.detailOverlay}><View style={styles.detail}><Pressable accessibilityLabel="关闭资源详情" hitSlop={8} onPress={() => setSelected(undefined)} style={styles.detailClose}><X color={colors.ink} size={18} /></Pressable><Text style={styles.resourceKind}>{kindLabel(selected.kind)} · {selected.provider}</Text><Text style={styles.detailTitle}>{selected.title}</Text><Text style={styles.detailSummary}>{selected.summary}</Text><Text style={styles.meta}>版本 {selected.version} · 许可 {selected.license}</Text><Text style={styles.citation}>{selected.citation}</Text><Pressable accessibilityLabel="打开资源来源" onPress={() => void Linking.openURL(selected.url)} style={styles.open}><ExternalLink color={colors.surface} size={16} /><Text style={styles.openText}>打开官方来源</Text></Pressable></View></View> : null}
  </View></Modal>;
}

function kindLabel(kind: Kind) { return ({ book: '书籍', course: '课程', tool: '工具', article: '文章', podcast: '播客' })[kind]; }

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background }, header: { height: 64, paddingHorizontal: 14, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', backgroundColor: colors.surface, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line }, icon: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' }, title: { color: colors.ink, fontSize: 16, fontWeight: '700', letterSpacing: 0 }, search: { margin: 15, paddingHorizontal: 11, minHeight: 40, flexDirection: 'row', alignItems: 'center', gap: 7, borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface }, searchInput: { flex: 1, color: colors.ink, fontSize: 13, letterSpacing: 0 }, kinds: { paddingHorizontal: 15, paddingBottom: 10, gap: 7 }, kind: { paddingHorizontal: 12, paddingVertical: 7, borderRadius: 5, backgroundColor: colors.surface }, kindActive: { backgroundColor: colors.evergreen }, kindText: { color: colors.muted, fontSize: 11, fontWeight: '700', letterSpacing: 0 }, kindActiveText: { color: colors.surface }, list: { padding: 15, gap: 9 }, card: { padding: 14, gap: 7, borderRadius: 7, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface }, cardTop: { flexDirection: 'row', justifyContent: 'space-between' }, resourceKind: { color: colors.evergreen, fontSize: 11, fontWeight: '700', letterSpacing: 0 }, provider: { color: colors.faint, fontSize: 11, letterSpacing: 0 }, resourceTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', lineHeight: 20, letterSpacing: 0 }, summary: { color: colors.muted, fontSize: 12, lineHeight: 18, letterSpacing: 0 }, topics: { flexDirection: 'row', gap: 8 }, topic: { color: colors.evergreen, fontSize: 10, letterSpacing: 0 }, loading: { flex: 1, alignItems: 'center', justifyContent: 'center' }, empty: { minHeight: 280, alignItems: 'center', justifyContent: 'center', gap: 8 }, emptyTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 }, muted: { color: colors.muted, fontSize: 12, letterSpacing: 0 }, detailOverlay: { position: 'absolute', top: 0, right: 0, bottom: 0, left: 0, justifyContent: 'flex-end', backgroundColor: 'rgba(21, 35, 30, 0.28)' }, detail: { padding: 21, gap: 10, borderTopLeftRadius: 12, borderTopRightRadius: 12, backgroundColor: colors.surface }, detailClose: { alignSelf: 'flex-end' }, detailTitle: { color: colors.ink, fontSize: 19, fontWeight: '800', lineHeight: 25, letterSpacing: 0 }, detailSummary: { color: colors.muted, fontSize: 13, lineHeight: 20, letterSpacing: 0 }, meta: { color: colors.faint, fontSize: 11, lineHeight: 17, letterSpacing: 0 }, citation: { color: colors.muted, fontSize: 11, lineHeight: 17, letterSpacing: 0 }, open: { minHeight: 40, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 6, borderRadius: 6, backgroundColor: colors.evergreen }, openText: { color: colors.surface, fontSize: 12, fontWeight: '700', letterSpacing: 0 }, pressed: { opacity: 0.65 },
});
