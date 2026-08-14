import { Download, LockKeyhole, ShieldCheck, X } from 'lucide-react-native';
import { useMemo, useState } from 'react';
import { Modal, Pressable, ScrollView, StyleSheet, Switch, Text, View } from 'react-native';

import { colors } from '../theme';
import { CommunityPost, GrowthEntry, Journey } from '../types';
import { type ProfileSection } from '../screens/ProfileScreen';

type Props = {
  section?: ProfileSection;
  visible: boolean;
  journeys: Journey[];
  entries: GrowthEntry[];
  savedPosts: CommunityPost[];
  onClose: () => void;
};

const titles: Record<ProfileSection, string> = {
  review: '成长回望',
  saved: '收藏与加入',
  creation: '创作中心',
  archive: '成长档案',
  privacy: '隐私与权限',
  settings: '设置与数据',
};

export function ProfileSectionModal({ section, visible, journeys, entries, savedPosts, onClose }: Props) {
  const [notifications, setNotifications] = useState(true);
  const [privateByDefault, setPrivateByDefault] = useState(true);
  const [analytics, setAnalytics] = useState(true);
  const exportPreview = useMemo(() => JSON.stringify({ journeys, entries }, null, 2), [entries, journeys]);
  if (!section) return null;
  return (
    <Modal animationType="slide" onRequestClose={onClose} visible={visible}>
      <View style={styles.screen}>
        <View style={styles.header}><Pressable accessibilityLabel="关闭" hitSlop={10} onPress={onClose} style={styles.close}><X color={colors.ink} size={22} /></Pressable><Text style={styles.title}>{titles[section]}</Text><View style={styles.close} /></View>
        <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
          {section === 'review' ? <Review entries={entries} journeys={journeys} /> : null}
          {section === 'saved' ? <Saved posts={savedPosts} journeys={journeys} /> : null}
          {section === 'creation' ? <Creation entries={entries} /> : null}
          {section === 'archive' ? <Archive entries={entries} journeys={journeys} /> : null}
          {section === 'privacy' ? <Privacy privateByDefault={privateByDefault} setPrivateByDefault={setPrivateByDefault} /> : null}
          {section === 'settings' ? <SettingsPanel analytics={analytics} exportPreview={exportPreview} notifications={notifications} setAnalytics={setAnalytics} setNotifications={setNotifications} /> : null}
        </ScrollView>
      </View>
    </Modal>
  );
}

function Review({ entries, journeys }: { entries: GrowthEntry[]; journeys: Journey[] }) {
  const active = journeys.filter((journey) => journey.status === 'active').length;
  const minutes = entries.reduce((sum, entry) => sum + (entry.duration_minutes ?? 0), 0);
  return <><Text style={styles.lead}>这一周，你正在按自己的节奏前进。</Text><View style={styles.statGrid}><Stat value={String(entries.length)} label="留下记录" /><Stat value={`${minutes} 分钟`} label="已投入" /><Stat value={String(active)} label="进行中路线" /></View><Text style={styles.section}>最近记录</Text>{entries.length ? entries.slice(0, 6).map((entry) => <View key={entry.id} style={styles.entry}><Text style={styles.entryBody}>{entry.body}</Text><Text style={styles.entryMeta}>{entry.location || '未标记地点'} · {entry.mood}</Text></View>) : <Empty text="完成一次行动后，回望会从这里开始。" />}</>;
}

function Saved({ posts, journeys }: { posts: CommunityPost[]; journeys: Journey[] }) {
  return <><Text style={styles.lead}>把值得参考的经验留在手边。</Text><Text style={styles.section}>收藏内容</Text>{posts.length ? posts.map((post) => <View key={post.id} style={styles.row}><Text numberOfLines={1} style={styles.rowTitle}>{post.title}</Text><Text style={styles.rowMeta}>{post.author_name} · {post.route_title}</Text></View>) : <Empty text="收藏的行记会出现在这里。" />}<Text style={styles.section}>加入的路线</Text>{journeys.length ? journeys.map((journey) => <View key={journey.id} style={styles.row}><Text numberOfLines={1} style={styles.rowTitle}>{journey.title}</Text><Text style={styles.rowMeta}>{journey.progress}% · {journey.status === 'active' ? '进行中' : journey.status === 'paused' ? '已暂停' : '已完成'}</Text></View>) : <Empty text="从发现页加入一条路线。" />}</>;
}

function Creation({ entries }: { entries: GrowthEntry[] }) {
  const published = entries.filter((entry) => entry.published);
  return <><Text style={styles.lead}>由真实行动产生的内容，才有可以传递的经验。</Text><View style={styles.statGrid}><Stat value={String(entries.length)} label="草稿与记录" /><Stat value={String(published.length)} label="已发布行记" /><Stat value={String(entries.length - published.length)} label="私密记录" /></View><Text style={styles.section}>已发布</Text>{published.length ? published.map((entry) => <View key={entry.id} style={styles.entry}><Text style={styles.entryBody}>{entry.body}</Text><Text style={styles.entryMeta}>行记 · 已发布</Text></View>) : <Empty text="发布的行记会在这里管理。" />}</>;
}

function Archive({ entries, journeys }: { entries: GrowthEntry[]; journeys: Journey[] }) {
  const completed = journeys.filter((journey) => journey.status === 'completed');
  return <><Text style={styles.lead}>所有走过的路，都会成为你的成长档案。</Text><View style={styles.statGrid}><Stat value={String(completed.length)} label="完成路线" /><Stat value={String(entries.length)} label="行动留痕" /><Stat value={String(journeys.length)} label="路线总数" /></View><Text style={styles.section}>已完成路线</Text>{completed.length ? completed.map((journey) => <View key={journey.id} style={styles.row}><Text style={styles.rowTitle}>{journey.title}</Text><Text style={styles.rowMeta}>{journey.duration_label} · 路线总结已生成</Text></View>) : <Empty text="完成一条路线后，会在这里留下总结。" />}</>;
}

function Privacy({ privateByDefault, setPrivateByDefault }: { privateByDefault: boolean; setPrivateByDefault: (value: boolean) => void }) {
  const [preciseLocation, setPreciseLocation] = useState(false);
  return <><View style={styles.notice}><ShieldCheck color={colors.evergreen} size={21} /><Text style={styles.noticeText}>路线、行动和记录默认仅自己可见。公开发布前会再次确认内容范围。</Text></View><SettingRow label="新记录默认私密" description="发布行记时单独选择公开" value={privateByDefault} onChange={setPrivateByDefault} /><SettingRow label="精确位置" description="仅在你主动添加地点时使用" value={preciseLocation} onChange={setPreciseLocation} /><View style={styles.danger}><LockKeyhole color={colors.coral} size={18} /><Text style={styles.dangerText}>账号注销和数据删除将在恢复期结束后执行。</Text></View></>;
}

function SettingsPanel({ notifications, setNotifications, analytics, setAnalytics, exportPreview }: { notifications: boolean; setNotifications: (value: boolean) => void; analytics: boolean; setAnalytics: (value: boolean) => void; exportPreview: string }) {
  const [showExport, setShowExport] = useState(false);
  return <><SettingRow label="行动提醒" description="在你安排的时间提醒一次" value={notifications} onChange={setNotifications} /><SettingRow label="匿名使用数据" description="用于改善推荐和稳定性" value={analytics} onChange={setAnalytics} /><Pressable onPress={() => setShowExport((value) => !value)} style={({ pressed }) => [styles.export, pressed && styles.pressed]}><Download color={colors.evergreen} size={19} /><Text style={styles.exportText}>查看数据导出预览</Text></Pressable>{showExport ? <Text selectable style={styles.exportPreview}>{exportPreview}</Text> : null}</>;
}

function SettingRow({ label, description, value, onChange }: { label: string; description: string; value: boolean; onChange: (value: boolean) => void }) {
  return <View style={styles.setting}><View style={styles.settingCopy}><Text style={styles.settingTitle}>{label}</Text><Text style={styles.settingText}>{description}</Text></View><Switch onValueChange={onChange} thumbColor={colors.surface} trackColor={{ false: colors.line, true: colors.evergreen }} value={value} /></View>;
}

function Stat({ value, label }: { value: string; label: string }) { return <View style={styles.stat}><Text style={styles.statValue}>{value}</Text><Text style={styles.statLabel}>{label}</Text></View>; }
function Empty({ text }: { text: string }) { return <Text style={styles.empty}>{text}</Text>; }

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { height: 64, paddingHorizontal: 16, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth, backgroundColor: colors.surface },
  close: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  title: { flex: 1, textAlign: 'center', color: colors.ink, fontSize: 16, fontWeight: '700', letterSpacing: 0 },
  content: { padding: 20, paddingBottom: 40 },
  lead: { color: colors.ink, fontSize: 17, lineHeight: 26, fontWeight: '700', letterSpacing: 0 },
  statGrid: { minHeight: 92, marginTop: 18, flexDirection: 'row', alignItems: 'center', borderRadius: 8, backgroundColor: colors.ink },
  stat: { flex: 1, minWidth: 0, alignItems: 'center', paddingHorizontal: 6 },
  statValue: { color: colors.surface, fontSize: 17, fontWeight: '800', letterSpacing: 0 },
  statLabel: { color: '#BBC1BD', fontSize: 10, marginTop: 5, textAlign: 'center', letterSpacing: 0 },
  section: { color: colors.ink, fontSize: 15, fontWeight: '700', marginTop: 27, marginBottom: 9, letterSpacing: 0 },
  entry: { padding: 14, marginBottom: 8, borderRadius: 7, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  entryBody: { color: colors.ink, fontSize: 14, lineHeight: 21, letterSpacing: 0 },
  entryMeta: { color: colors.faint, fontSize: 11, marginTop: 7, letterSpacing: 0 },
  row: { paddingVertical: 14, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  rowTitle: { color: colors.ink, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  rowMeta: { color: colors.muted, fontSize: 11, marginTop: 4, letterSpacing: 0 },
  empty: { paddingVertical: 20, textAlign: 'center', color: colors.faint, fontSize: 13, lineHeight: 20, letterSpacing: 0 },
  notice: { padding: 15, flexDirection: 'row', gap: 10, borderRadius: 7, backgroundColor: colors.evergreenSoft },
  noticeText: { flex: 1, color: colors.muted, fontSize: 13, lineHeight: 20, letterSpacing: 0 },
  setting: { minHeight: 75, flexDirection: 'row', alignItems: 'center', gap: 12, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  settingCopy: { flex: 1, minWidth: 0 },
  settingTitle: { color: colors.ink, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  settingText: { color: colors.muted, fontSize: 11, lineHeight: 17, marginTop: 3, letterSpacing: 0 },
  danger: { padding: 14, marginTop: 24, flexDirection: 'row', gap: 10, borderRadius: 7, backgroundColor: colors.coralSoft },
  dangerText: { flex: 1, color: colors.coral, fontSize: 12, lineHeight: 19, letterSpacing: 0 },
  export: { minHeight: 56, marginTop: 17, paddingHorizontal: 14, flexDirection: 'row', alignItems: 'center', gap: 10, borderRadius: 7, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  exportText: { color: colors.evergreen, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  exportPreview: { marginTop: 12, padding: 12, color: colors.muted, borderRadius: 7, backgroundColor: colors.surface, fontFamily: 'monospace', fontSize: 10, lineHeight: 15 },
  pressed: { opacity: 0.62 },
});
