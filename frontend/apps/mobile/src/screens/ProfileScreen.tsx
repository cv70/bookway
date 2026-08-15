import { Award, BookOpenText, BookMarked, ChevronRight, FilePenLine, LockKeyhole, MessageSquarePlus, Settings, ShieldCheck, Sparkles, UserRound } from 'lucide-react-native';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { ScreenHeader } from '../components/ScreenHeader';
import { colors } from '../theme';
import { AccountProfile, GrowthEntry, Journey, Today } from '../types';

const links = [
  { key: 'review', label: '成长回望', icon: Sparkles },
  { key: 'saved', label: '收藏与加入', icon: BookMarked },
  { key: 'creation', label: '创作中心', icon: FilePenLine },
  { key: 'archive', label: '成长档案', icon: Award },
  { key: 'privacy', label: '隐私与权限', icon: LockKeyhole },
  { key: 'settings', label: '设置与数据', icon: Settings },
] as const;

export type ProfileSection = typeof links[number]['key'];

export function ProfileScreen({ profile, journeys, today, entries, moderator, onOpenSection, onOpenLibrary, onOpenFeedback, onOpenModeration }: { profile: AccountProfile; journeys: Journey[]; today: Today; entries: GrowthEntry[]; moderator: boolean; onOpenSection: (section: ProfileSection) => void; onOpenLibrary: () => void; onOpenFeedback: () => void; onOpenModeration: () => void }) {
  const activeJourneys = journeys.filter((journey) => journey.status === 'active').length;
  return (
    <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
      <ScreenHeader title="我的" />
      <View style={styles.identity}>
        <View style={styles.avatar}><UserRound color={colors.evergreen} size={34} /></View>
        <View style={styles.identityCopy}>
          <Text style={styles.name}>{profile.display_name}</Text>
          <Text style={styles.caption}>{profile.bio || '在万卷与山河之间，成为自己'}</Text>
        </View>
      </View>
      <View style={styles.stats}>
        <ProfileStat label="进行中路线" value={String(activeJourneys)} />
        <ProfileStat label="今日完成" value={String(today.completed)} />
        <ProfileStat label="留下记录" value={String(entries.length)} />
      </View>
      <Text style={styles.sectionTitle}>成长资产</Text>
      <View style={styles.links}>
        <Pressable onPress={onOpenLibrary} style={({ pressed }) => [styles.link, pressed && styles.pressed]}>
          <View style={styles.linkIcon}><BookOpenText color={colors.evergreen} size={19} /></View>
          <Text style={styles.linkText}>我的书架</Text>
          <ChevronRight color={colors.faint} size={18} />
        </Pressable>
        {links.map(({ key, label, icon: Icon }) => (
          <Pressable key={key} onPress={() => onOpenSection(key)} style={({ pressed }) => [styles.link, pressed && styles.pressed]}>
            <View style={styles.linkIcon}><Icon color={colors.evergreen} size={19} /></View>
            <Text style={styles.linkText}>{label}</Text>
            <ChevronRight color={colors.faint} size={18} />
          </Pressable>
        ))}
        <Pressable onPress={onOpenFeedback} style={({ pressed }) => [styles.link, pressed && styles.pressed]}>
          <View style={styles.linkIcon}><MessageSquarePlus color={colors.evergreen} size={19} /></View>
          <Text style={styles.linkText}>意见反馈</Text>
          <ChevronRight color={colors.faint} size={18} />
        </Pressable>
        {moderator ? <Pressable accessibilityLabel="打开评论审核工作台" onPress={onOpenModeration} style={({ pressed }) => [styles.link, styles.moderationLink, pressed && styles.pressed]}>
          <View style={[styles.linkIcon, styles.moderationIcon]}><ShieldCheck color={colors.gold} size={19} /></View>
          <View style={styles.linkCopy}><Text style={styles.linkText}>评论审核工作台</Text><Text style={styles.linkHint}>仅限受授权审核角色</Text></View>
          <ChevronRight color={colors.faint} size={18} />
        </Pressable> : null}
      </View>
    </ScrollView>
  );
}

function ProfileStat({ label, value }: { label: string; value: string }) {
  return (
    <View style={styles.stat}>
      <Text style={styles.statValue}>{value}</Text>
      <Text style={styles.statLabel}>{label}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 30 },
  identity: { flexDirection: 'row', alignItems: 'center', gap: 14, paddingHorizontal: 20, paddingVertical: 16 },
  avatar: { width: 64, height: 64, borderRadius: 32, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.evergreenSoft },
  identityCopy: { flex: 1, minWidth: 0 },
  name: { color: colors.ink, fontSize: 20, fontWeight: '700', letterSpacing: 0 },
  caption: { color: colors.muted, fontSize: 12, lineHeight: 18, marginTop: 4, letterSpacing: 0 },
  stats: { minHeight: 90, flexDirection: 'row', alignItems: 'center', marginTop: 10, backgroundColor: colors.ink },
  stat: { flex: 1, alignItems: 'center' },
  statValue: { color: colors.surface, fontSize: 19, fontWeight: '800', letterSpacing: 0 },
  statLabel: { color: '#BBC1BD', fontSize: 10, marginTop: 4, letterSpacing: 0 },
  sectionTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', marginHorizontal: 20, marginTop: 26, marginBottom: 10, letterSpacing: 0 },
  links: { backgroundColor: colors.surface, borderTopColor: colors.line, borderBottomColor: colors.line, borderTopWidth: StyleSheet.hairlineWidth, borderBottomWidth: StyleSheet.hairlineWidth },
  link: { minHeight: 58, marginLeft: 20, paddingRight: 20, flexDirection: 'row', alignItems: 'center', gap: 12, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  pressed: { opacity: 0.55 },
  linkIcon: { width: 30, height: 30, borderRadius: 6, backgroundColor: colors.evergreenSoft, alignItems: 'center', justifyContent: 'center' },
  moderationLink: { backgroundColor: '#FFFBF2' },
  moderationIcon: { backgroundColor: colors.goldSoft },
  linkCopy: { flex: 1, minWidth: 0 },
  linkText: { flex: 1, color: colors.ink, fontSize: 14, fontWeight: '600', letterSpacing: 0 },
  linkHint: { color: colors.faint, fontSize: 10, marginTop: 2, letterSpacing: 0 },
});
