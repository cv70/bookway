import { Download, LockKeyhole, ShieldCheck, X } from 'lucide-react-native';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Modal, Pressable, ScrollView, StyleSheet, Switch, Text, TextInput, View } from 'react-native';

import { colors } from '../theme';
import { appealPost, getMyAppeals, getMyPosts, getReminderPreferences, updateReminderPreferences } from '../api/client';
import { CommunityPost, ContentAppeal, GrowthEntry, Journey, OwnedContent, ReminderPreferences, ReviewAdjustmentSuggestion, WeeklyReview } from '../types';
import { localScheduleContext } from '../utils/scheduling';
import { type ProfileSection } from '../screens/ProfileScreen';

type Props = {
  section?: ProfileSection;
  visible: boolean;
  journeys: Journey[];
  entries: GrowthEntry[];
  savedPosts: CommunityPost[];
  review?: WeeklyReview;
  onApplyReviewSuggestion: (suggestion: ReviewAdjustmentSuggestion) => void;
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

export function ProfileSectionModal({ section, visible, journeys, entries, savedPosts, review, onApplyReviewSuggestion, onClose }: Props) {
  const [notifications, setNotifications] = useState(false);
  const [reminderPreferences, setReminderPreferences] = useState<ReminderPreferences>();
  const [remindersLoading, setRemindersLoading] = useState(false);
  const [remindersSaving, setRemindersSaving] = useState(false);
  const [reminderError, setReminderError] = useState<string>();
  const [privateByDefault, setPrivateByDefault] = useState(true);
  const [analytics, setAnalytics] = useState(true);
  const [ownContents, setOwnContents] = useState<OwnedContent[]>([]);
  const [appeals, setAppeals] = useState<ContentAppeal[]>([]);
  const [ownContentsNextCursor, setOwnContentsNextCursor] = useState<string>();
  const [appealsNextCursor, setAppealsNextCursor] = useState<string>();
  const [contentManagementLoading, setContentManagementLoading] = useState(false);
  const [ownContentsLoadingMore, setOwnContentsLoadingMore] = useState(false);
  const [appealsLoadingMore, setAppealsLoadingMore] = useState(false);
  const [contentManagementError, setContentManagementError] = useState<string>();
  const [ownContentsLoadMoreError, setOwnContentsLoadMoreError] = useState<string>();
  const [appealsLoadMoreError, setAppealsLoadMoreError] = useState<string>();
  const [appealSubmittingContentId, setAppealSubmittingContentId] = useState<string>();
  const [contentManagementRefresh, setContentManagementRefresh] = useState(0);
  const contentManagementRequestVersionRef = useRef(0);
  const ownContentsLoadMoreVersionRef = useRef<number | undefined>(undefined);
  const appealsLoadMoreVersionRef = useRef<number | undefined>(undefined);
  const exportPreview = useMemo(() => JSON.stringify({ journeys, entries }, null, 2), [entries, journeys]);
  useEffect(() => {
    if (!visible || section !== 'settings') return undefined;
    let active = true;
    setRemindersLoading(true);
    setReminderError(undefined);
    void getReminderPreferences()
      .then((preferences) => {
        if (!active) return;
        setReminderPreferences(preferences);
        setNotifications(preferences.enabled);
      })
      .catch(() => {
        if (active) setReminderError('暂时无法读取提醒设置。');
      })
      .finally(() => {
        if (active) setRemindersLoading(false);
      });
    return () => { active = false; };
  }, [section, visible]);
  useEffect(() => {
    if (!visible || section !== 'creation') return undefined;
    const requestVersion = contentManagementRequestVersionRef.current + 1;
    contentManagementRequestVersionRef.current = requestVersion;
    ownContentsLoadMoreVersionRef.current = undefined;
    appealsLoadMoreVersionRef.current = undefined;
    let active = true;
    setContentManagementLoading(true);
    setContentManagementError(undefined);
    setOwnContentsLoadMoreError(undefined);
    setAppealsLoadMoreError(undefined);
    setOwnContentsLoadingMore(false);
    setAppealsLoadingMore(false);
    setOwnContents([]);
    setAppeals([]);
    setOwnContentsNextCursor(undefined);
    setAppealsNextCursor(undefined);
    void Promise.allSettled([getMyPosts(), getMyAppeals()])
      .then(([contentResult, appealResult]) => {
        if (!active || contentManagementRequestVersionRef.current !== requestVersion) return;
        if (contentResult.status === 'fulfilled') {
          setOwnContents(contentResult.value.items);
          setOwnContentsNextCursor(contentResult.value.next_cursor ?? undefined);
        }
        if (appealResult.status === 'fulfilled') {
          setAppeals(appealResult.value.items);
          setAppealsNextCursor(appealResult.value.next_cursor ?? undefined);
        }
        if (contentResult.status === 'rejected' || appealResult.status === 'rejected') {
          setContentManagementError('部分内容状态暂时无法读取，可稍后重试。');
        }
      })
      .finally(() => {
        if (active && contentManagementRequestVersionRef.current === requestVersion) setContentManagementLoading(false);
      });
    return () => {
      active = false;
      if (contentManagementRequestVersionRef.current === requestVersion) contentManagementRequestVersionRef.current += 1;
    };
  }, [contentManagementRefresh, section, visible]);
  const setNotificationsPreference = (enabled: boolean) => {
    const current = reminderPreferences;
    const timezone = localScheduleContext().timezone;
    const next = {
      enabled,
      lead_minutes: current?.lead_minutes ?? 0,
      timezone,
      quiet_hours_start: current?.quiet_hours_start,
      quiet_hours_end: current?.quiet_hours_end,
    };
    setNotifications(enabled);
    setRemindersSaving(true);
    setReminderError(undefined);
    void updateReminderPreferences(next)
      .then((preferences) => {
        setReminderPreferences(preferences);
        setNotifications(preferences.enabled);
      })
      .catch(() => {
        setReminderPreferences(current);
        setNotifications(current?.enabled ?? false);
        setReminderError('提醒设置未保存，请稍后重试。');
      })
      .finally(() => setRemindersSaving(false));
  };
  const submitAppeal = async (contentId: string, details: string) => {
    setAppealSubmittingContentId(contentId);
    setContentManagementError(undefined);
    try {
      const appeal = await appealPost(contentId, details);
      setAppeals((current) => [appeal, ...current.filter((item) => item.id !== appeal.id)]);
    } catch (error) {
      setContentManagementError('申诉未能提交，请检查网络后重试。');
      throw error;
    } finally {
      setAppealSubmittingContentId(undefined);
    }
  };
  const loadMoreOwnContents = async () => {
    const cursor = ownContentsNextCursor;
    const requestVersion = contentManagementRequestVersionRef.current;
    if (!cursor || ownContentsLoadMoreVersionRef.current === requestVersion) return;
    ownContentsLoadMoreVersionRef.current = requestVersion;
    setOwnContentsLoadingMore(true);
    setOwnContentsLoadMoreError(undefined);
    try {
      const page = await getMyPosts(cursor);
      if (contentManagementRequestVersionRef.current !== requestVersion) return;
      setOwnContents((current) => mergeOwnedContents(current, page.items));
      setOwnContentsNextCursor(page.next_cursor ?? undefined);
    } catch {
      if (contentManagementRequestVersionRef.current === requestVersion) {
        setOwnContentsLoadMoreError('更多内容状态暂时无法读取。');
      }
    } finally {
      if (ownContentsLoadMoreVersionRef.current === requestVersion) {
        ownContentsLoadMoreVersionRef.current = undefined;
        setOwnContentsLoadingMore(false);
      }
    }
  };
  const loadMoreAppeals = async () => {
    const cursor = appealsNextCursor;
    const requestVersion = contentManagementRequestVersionRef.current;
    if (!cursor || appealsLoadMoreVersionRef.current === requestVersion) return;
    appealsLoadMoreVersionRef.current = requestVersion;
    setAppealsLoadingMore(true);
    setAppealsLoadMoreError(undefined);
    try {
      const page = await getMyAppeals(cursor);
      if (contentManagementRequestVersionRef.current !== requestVersion) return;
      setAppeals((current) => mergeAppeals(current, page.items));
      setAppealsNextCursor(page.next_cursor ?? undefined);
    } catch {
      if (contentManagementRequestVersionRef.current === requestVersion) {
        setAppealsLoadMoreError('更多申诉记录暂时无法读取。');
      }
    } finally {
      if (appealsLoadMoreVersionRef.current === requestVersion) {
        appealsLoadMoreVersionRef.current = undefined;
        setAppealsLoadingMore(false);
      }
    }
  };
  if (!section) return null;
  return (
    <Modal animationType="slide" onRequestClose={onClose} visible={visible}>
      <View style={styles.screen}>
        <View style={styles.header}><Pressable accessibilityLabel="关闭" hitSlop={10} onPress={onClose} style={styles.close}><X color={colors.ink} size={22} /></Pressable><Text style={styles.title}>{titles[section]}</Text><View style={styles.close} /></View>
        <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
          {section === 'review' ? <Review entries={entries} journeys={journeys} onApplySuggestion={onApplyReviewSuggestion} review={review} /> : null}
          {section === 'saved' ? <Saved posts={savedPosts} journeys={journeys} /> : null}
          {section === 'creation' ? <Creation appeals={appeals} appealsLoadMoreError={appealsLoadMoreError} appealsLoadingMore={appealsLoadingMore} appealsNextCursor={appealsNextCursor} contentManagementError={contentManagementError} contentManagementLoading={contentManagementLoading} contents={ownContents} contentsLoadMoreError={ownContentsLoadMoreError} contentsLoadingMore={ownContentsLoadingMore} contentsNextCursor={ownContentsNextCursor} entries={entries} onLoadMoreAppeals={() => void loadMoreAppeals()} onLoadMoreContents={() => void loadMoreOwnContents()} onRefreshContentManagement={() => setContentManagementRefresh((value) => value + 1)} onSubmitAppeal={submitAppeal} submittingContentId={appealSubmittingContentId} /> : null}
          {section === 'archive' ? <Archive entries={entries} journeys={journeys} /> : null}
          {section === 'privacy' ? <Privacy privateByDefault={privateByDefault} setPrivateByDefault={setPrivateByDefault} /> : null}
          {section === 'settings' ? <SettingsPanel analytics={analytics} exportPreview={exportPreview} notificationStatus={reminderError ?? (remindersLoading ? '正在读取提醒设置…' : remindersSaving ? '正在保存提醒设置…' : undefined)} notifications={notifications} remindersBusy={remindersLoading || remindersSaving} setAnalytics={setAnalytics} setNotifications={setNotificationsPreference} /> : null}
        </ScrollView>
      </View>
    </Modal>
  );
}

function Review({ entries, journeys, review, onApplySuggestion }: { entries: GrowthEntry[]; journeys: Journey[]; review?: WeeklyReview; onApplySuggestion: (suggestion: ReviewAdjustmentSuggestion) => void }) {
  const active = journeys.filter((journey) => journey.status === 'active').length;
  const minutes = entries.reduce((sum, entry) => sum + (entry.duration_minutes ?? 0), 0);
  const prompts = review?.reflection_prompts ?? [];
  const suggestions = review?.adjustment_suggestions ?? [];
  return <>
    <Text style={styles.lead}>这一周，你正在按自己的节奏前进。</Text>
    <View style={styles.statGrid}><Stat value={String(review?.entry_count ?? entries.length)} label="留下记录" /><Stat value={`${review?.focus_minutes ?? minutes} 分钟`} label="已投入" /><Stat value={String(review?.active_journeys ?? active)} label="进行中路线" /></View>
    {suggestions.length ? <><Text style={styles.section}>可以尝试的调整</Text>{suggestions.map((suggestion) => <View key={`${suggestion.kind}-${suggestion.title}`} style={styles.suggestion}><Text style={styles.suggestionTitle}>{suggestion.title}</Text><Text style={styles.suggestionText}>{suggestion.rationale}</Text><Pressable accessibilityLabel={`采用调整：${suggestion.title}`} onPress={() => onApplySuggestion(suggestion)} style={({ pressed }) => [styles.suggestionApply, pressed && styles.pressed]}><Text style={styles.suggestionApplyText}>采用这个调整</Text></Pressable></View>)}</> : null}
    {prompts.length ? <><Text style={styles.section}>回望提示</Text>{prompts.map((prompt) => <View key={prompt} style={styles.prompt}><Text style={styles.promptText}>{prompt}</Text></View>)}</> : null}
    <Text style={styles.section}>最近记录</Text>{entries.length ? entries.slice(0, 6).map((entry) => <View key={entry.id} style={styles.entry}><Text style={styles.entryBody}>{entry.body}</Text><Text style={styles.entryMeta}>{entry.location || '未标记地点'} · {entry.mood}</Text></View>) : <Empty text="完成一次行动后，回望会从这里开始。" />}
  </>;
}

function Saved({ posts, journeys }: { posts: CommunityPost[]; journeys: Journey[] }) {
  return <><Text style={styles.lead}>把值得参考的经验留在手边。</Text><Text style={styles.section}>收藏内容</Text>{posts.length ? posts.map((post) => <View key={post.id} style={styles.row}><Text numberOfLines={1} style={styles.rowTitle}>{post.title}</Text><Text style={styles.rowMeta}>{post.author_name} · {post.route_title}</Text></View>) : <Empty text="收藏的行记会出现在这里。" />}<Text style={styles.section}>加入的路线</Text>{journeys.length ? journeys.map((journey) => <View key={journey.id} style={styles.row}><Text numberOfLines={1} style={styles.rowTitle}>{journey.title}</Text><Text style={styles.rowMeta}>{journey.progress}% · {journey.status === 'active' ? '进行中' : journey.status === 'paused' ? '已暂停' : '已完成'}</Text></View>) : <Empty text="从发现页加入一条路线。" />}</>;
}

function Creation({ appeals, appealsLoadMoreError, appealsLoadingMore, appealsNextCursor, contentManagementError, contentManagementLoading, contents, contentsLoadMoreError, contentsLoadingMore, contentsNextCursor, entries, onLoadMoreAppeals, onLoadMoreContents, onRefreshContentManagement, onSubmitAppeal, submittingContentId }: { appeals: ContentAppeal[]; appealsLoadMoreError?: string; appealsLoadingMore: boolean; appealsNextCursor?: string; contentManagementError?: string; contentManagementLoading: boolean; contents: OwnedContent[]; contentsLoadMoreError?: string; contentsLoadingMore: boolean; contentsNextCursor?: string; entries: GrowthEntry[]; onLoadMoreAppeals: () => void; onLoadMoreContents: () => void; onRefreshContentManagement: () => void; onSubmitAppeal: (contentId: string, details: string) => Promise<void>; submittingContentId?: string }) {
  const published = entries.filter((entry) => entry.published);
  return <><Text style={styles.lead}>由真实行动产生的内容，才有可以传递的经验。</Text><View style={styles.statGrid}><Stat value={String(entries.length)} label="草稿与记录" /><Stat value={String(published.length)} label="已发布行记" /><Stat value={String(entries.length - published.length)} label="私密记录" /></View><Text style={styles.section}>已发布</Text>{published.length ? published.map((entry) => <View key={entry.id} style={styles.entry}><Text style={styles.entryBody}>{entry.body}</Text><Text style={styles.entryMeta}>行记 · 已发布</Text></View>) : <Empty text="发布的行记会在这里管理。" />}<ContentAppeals appeals={appeals} appealsLoadMoreError={appealsLoadMoreError} appealsLoadingMore={appealsLoadingMore} appealsNextCursor={appealsNextCursor} contentLoadMoreError={contentsLoadMoreError} contents={contents} contentsLoadingMore={contentsLoadingMore} contentsNextCursor={contentsNextCursor} error={contentManagementError} loading={contentManagementLoading} onLoadMoreAppeals={onLoadMoreAppeals} onLoadMoreContents={onLoadMoreContents} onRefresh={onRefreshContentManagement} onSubmit={onSubmitAppeal} submittingContentId={submittingContentId} /></>;
}

function ContentAppeals({ appeals, appealsLoadMoreError, appealsLoadingMore, appealsNextCursor, contentLoadMoreError, contents, contentsLoadingMore, contentsNextCursor, loading, error, submittingContentId, onLoadMoreAppeals, onLoadMoreContents, onRefresh, onSubmit }: { appeals: ContentAppeal[]; appealsLoadMoreError?: string; appealsLoadingMore: boolean; appealsNextCursor?: string; contentLoadMoreError?: string; contents: OwnedContent[]; contentsLoadingMore: boolean; contentsNextCursor?: string; loading: boolean; error?: string; submittingContentId?: string; onLoadMoreAppeals: () => void; onLoadMoreContents: () => void; onRefresh: () => void; onSubmit: (contentId: string, details: string) => Promise<void> }) {
  const restrictedContents = contents.filter((content) => content.status === 'restricted');
  const latestAppeals = new Map<string, ContentAppeal>();
  [...appeals].sort((left, right) => right.created_at.localeCompare(left.created_at)).forEach((appeal) => {
    if (!latestAppeals.has(appeal.content_id)) latestAppeals.set(appeal.content_id, appeal);
  });
  return <><Text style={styles.section}>内容处置与申诉</Text><View style={styles.appealNotice}><ShieldCheck color={colors.evergreen} size={19} /><Text style={styles.appealNoticeText}>这里仅显示你的内容状态和申诉记录。受限内容不会向其他用户公开。</Text></View>{loading ? <Text accessibilityLiveRegion="polite" style={styles.appealLoading}>正在读取内容状态…</Text> : null}{error ? <Pressable accessibilityRole="button" onPress={onRefresh} style={({ pressed }) => [styles.appealRetry, pressed && styles.pressed]}><Text style={styles.appealRetryText}>{error} 点击重试</Text></Pressable> : null}{!loading && restrictedContents.length === 0 ? <Empty text={contentsNextCursor ? '当前页没有需要申诉的受限内容。' : '目前没有需要申诉的受限内容。'} /> : restrictedContents.map((content) => <RestrictedContentCard appeal={latestAppeals.get(content.id)} content={content} key={content.id} onSubmit={onSubmit} submitting={submittingContentId === content.id} />)}{contentsNextCursor ? <LoadMore error={contentLoadMoreError} label="加载更多内容状态" loading={contentsLoadingMore} onPress={onLoadMoreContents} /> : null}{appeals.length || appealsNextCursor ? <><Text style={styles.appealHistorySection}>申诉记录</Text>{[...appeals].sort((left, right) => right.created_at.localeCompare(left.created_at)).map((appeal) => <AppealHistoryCard appeal={appeal} content={contents.find((item) => item.id === appeal.content_id)} key={appeal.id} />)}{appealsNextCursor ? <LoadMore error={appealsLoadMoreError} label="加载更多申诉记录" loading={appealsLoadingMore} onPress={onLoadMoreAppeals} /> : null}</> : null}</>;
}

function LoadMore({ error, label, loading, onPress }: { error?: string; label: string; loading: boolean; onPress: () => void }) {
  return <Pressable accessibilityRole="button" disabled={loading} onPress={onPress} style={({ pressed }) => [styles.appealLoadMore, (pressed || loading) && styles.pressed]}><Text accessibilityLiveRegion="polite" style={styles.appealLoadMoreText}>{loading ? '正在加载…' : error ? `${error} 点击重试` : label}</Text></Pressable>;
}

function RestrictedContentCard({ content, appeal, submitting, onSubmit }: { content: OwnedContent; appeal?: ContentAppeal; submitting: boolean; onSubmit: (contentId: string, details: string) => Promise<void> }) {
  const [details, setDetails] = useState('');
  const [submitError, setSubmitError] = useState<string>();
  const activeAppeal = appeal?.status === 'pending' || appeal?.status === 'reviewing';
  const submit = async () => {
    if (!details.trim() || submitting) return;
    setSubmitError(undefined);
    try {
      await onSubmit(content.id, details);
      setDetails('');
    } catch {
      setSubmitError('申诉未提交，草稿已保留。');
    }
  };
  return <View style={styles.restrictedContent}><View style={styles.restrictedHeading}><View style={styles.restrictedMarker} /><View style={styles.restrictedCopy}><Text numberOfLines={2} style={styles.restrictedTitle}>{content.post.title}</Text><Text style={styles.restrictedMeta}>当前状态：已受限</Text></View></View>{appeal ? <View style={styles.appealDecision}><Text style={styles.appealDecisionStatus}>{appealStatusLabel(appeal.status)}</Text><Text style={styles.appealDecisionText}>{appeal.resolution || (activeAppeal ? '已收到申诉，正在核验相关内容。' : '审核结果将在这里说明。')}</Text></View> : null}{activeAppeal ? null : <><TextInput accessibilityLabel={`申诉说明：${content.post.title}`} maxLength={1000} multiline onChangeText={(value) => { setDetails(value); setSubmitError(undefined); }} placeholder={appeal ? '补充新的事实或材料后再次申诉' : '说明你认为原处置需要复核的原因'} placeholderTextColor={colors.faint} style={styles.appealInput} textAlignVertical="top" value={details} /><View style={styles.appealSubmitRow}><Text style={styles.appealCharacterCount}>{details.length}/1000</Text><Pressable accessibilityLabel="提交内容申诉" disabled={!details.trim() || submitting} onPress={() => void submit()} style={[styles.appealSubmit, (!details.trim() || submitting) && styles.appealSubmitDisabled]}><Text style={styles.appealSubmitText}>{submitting ? '提交中…' : appeal ? '再次申诉' : '提交申诉'}</Text></Pressable></View>{submitError ? <Text accessibilityLiveRegion="polite" style={styles.appealSubmitError}>{submitError}</Text> : null}</>}</View>;
}

function AppealHistoryCard({ appeal, content }: { appeal: ContentAppeal; content?: OwnedContent }) {
  return <View style={styles.appealHistory}><View style={styles.appealHistoryHeader}><Text numberOfLines={1} style={styles.appealHistoryTitle}>{content?.post.title || `内容 ${appeal.content_id.slice(0, 8)}`}</Text><Text style={styles.appealHistoryStatus}>{appealStatusLabel(appeal.status)}</Text></View><Text numberOfLines={2} style={styles.appealHistoryDetails}>{appeal.details}</Text>{appeal.resolution ? <Text style={styles.appealHistoryResolution}>审核说明：{appeal.resolution}</Text> : null}<Text style={styles.appealHistoryDate}>{formatAppealDate(appeal.updated_at || appeal.created_at)}</Text></View>;
}

function appealStatusLabel(status: ContentAppeal['status']) {
  if (status === 'pending') return '待受理';
  if (status === 'reviewing') return '复核中';
  return status === 'resolved' ? '已处理' : '未通过';
}

function formatAppealDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '时间待确认' : `${date.getFullYear()} 年 ${date.getMonth() + 1} 月 ${date.getDate()} 日`;
}

function mergeOwnedContents(current: OwnedContent[], incoming: OwnedContent[]) {
  const knownIds = new Set(current.map((content) => content.id));
  return [...current, ...incoming.filter((content) => !knownIds.has(content.id))];
}

function mergeAppeals(current: ContentAppeal[], incoming: ContentAppeal[]) {
  const knownIds = new Set(current.map((appeal) => appeal.id));
  return [...current, ...incoming.filter((appeal) => !knownIds.has(appeal.id))];
}

function Archive({ entries, journeys }: { entries: GrowthEntry[]; journeys: Journey[] }) {
  const completed = journeys.filter((journey) => journey.status === 'completed');
  return <><Text style={styles.lead}>所有走过的路，都会成为你的成长档案。</Text><View style={styles.statGrid}><Stat value={String(completed.length)} label="完成路线" /><Stat value={String(entries.length)} label="行动留痕" /><Stat value={String(journeys.length)} label="路线总数" /></View><Text style={styles.section}>已完成路线</Text>{completed.length ? completed.map((journey) => <View key={journey.id} style={styles.row}><Text style={styles.rowTitle}>{journey.title}</Text><Text style={styles.rowMeta}>{journey.duration_label} · 路线总结已生成</Text></View>) : <Empty text="完成一条路线后，会在这里留下总结。" />}</>;
}

function Privacy({ privateByDefault, setPrivateByDefault }: { privateByDefault: boolean; setPrivateByDefault: (value: boolean) => void }) {
  const [preciseLocation, setPreciseLocation] = useState(false);
  return <><View style={styles.notice}><ShieldCheck color={colors.evergreen} size={21} /><Text style={styles.noticeText}>路线、行动和记录默认仅自己可见。公开发布前会再次确认内容范围。</Text></View><SettingRow label="新记录默认私密" description="发布行记时单独选择公开" value={privateByDefault} onChange={setPrivateByDefault} /><SettingRow label="精确位置" description="仅在你主动添加地点时使用" value={preciseLocation} onChange={setPreciseLocation} /><View style={styles.danger}><LockKeyhole color={colors.coral} size={18} /><Text style={styles.dangerText}>账号注销和数据删除将在恢复期结束后执行。</Text></View></>;
}

function SettingsPanel({ notifications, setNotifications, analytics, setAnalytics, exportPreview, remindersBusy, notificationStatus }: { notifications: boolean; setNotifications: (value: boolean) => void; analytics: boolean; setAnalytics: (value: boolean) => void; exportPreview: string; remindersBusy: boolean; notificationStatus?: string }) {
  const [showExport, setShowExport] = useState(false);
  return <><SettingRow disabled={remindersBusy} label="行动提醒" description="保存提醒偏好；启用系统通知后可按安排提醒" value={notifications} onChange={setNotifications} />{notificationStatus ? <Text style={styles.settingStatus}>{notificationStatus}</Text> : null}<SettingRow label="匿名使用数据" description="用于改善推荐和稳定性" value={analytics} onChange={setAnalytics} /><Pressable onPress={() => setShowExport((value) => !value)} style={({ pressed }) => [styles.export, pressed && styles.pressed]}><Download color={colors.evergreen} size={19} /><Text style={styles.exportText}>查看数据导出预览</Text></Pressable>{showExport ? <Text selectable style={styles.exportPreview}>{exportPreview}</Text> : null}</>;
}

function SettingRow({ label, description, value, onChange, disabled = false }: { label: string; description: string; value: boolean; onChange: (value: boolean) => void; disabled?: boolean }) {
  return <View style={styles.setting}><View style={styles.settingCopy}><Text style={styles.settingTitle}>{label}</Text><Text style={styles.settingText}>{description}</Text></View><Switch disabled={disabled} onValueChange={onChange} thumbColor={colors.surface} trackColor={{ false: colors.line, true: colors.evergreen }} value={value} /></View>;
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
  suggestion: { padding: 14, marginBottom: 8, borderRadius: 7, borderWidth: 1, borderColor: colors.evergreen, backgroundColor: colors.evergreenSoft },
  suggestionTitle: { color: colors.ink, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  suggestionText: { color: colors.muted, fontSize: 12, lineHeight: 19, marginTop: 5, letterSpacing: 0 },
  suggestionApply: { alignSelf: 'flex-start', minHeight: 34, marginTop: 11, paddingHorizontal: 11, justifyContent: 'center', borderRadius: 5, backgroundColor: colors.evergreen },
  suggestionApplyText: { color: colors.surface, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  entry: { padding: 14, marginBottom: 8, borderRadius: 7, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  entryBody: { color: colors.ink, fontSize: 14, lineHeight: 21, letterSpacing: 0 },
  entryMeta: { color: colors.faint, fontSize: 11, marginTop: 7, letterSpacing: 0 },
  appealNotice: { padding: 14, flexDirection: 'row', gap: 9, borderRadius: 7, backgroundColor: colors.evergreenSoft },
  appealNoticeText: { flex: 1, color: colors.muted, fontSize: 12, lineHeight: 19, letterSpacing: 0 },
  appealLoading: { paddingVertical: 15, color: colors.muted, fontSize: 12, textAlign: 'center', letterSpacing: 0 },
  appealRetry: { marginTop: 10, padding: 12, borderRadius: 7, backgroundColor: colors.coralSoft },
  appealRetryText: { color: colors.coral, fontSize: 12, lineHeight: 18, textAlign: 'center', letterSpacing: 0 },
  appealLoadMore: { marginTop: 10, minHeight: 42, paddingHorizontal: 12, justifyContent: 'center', borderRadius: 7, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  appealLoadMoreText: { color: colors.evergreen, fontSize: 12, fontWeight: '700', lineHeight: 18, textAlign: 'center', letterSpacing: 0 },
  restrictedContent: { marginTop: 10, padding: 14, borderRadius: 7, borderWidth: 1, borderColor: colors.coral, backgroundColor: colors.surface },
  restrictedHeading: { flexDirection: 'row', gap: 10, alignItems: 'flex-start' },
  restrictedMarker: { width: 4, minHeight: 35, borderRadius: 2, backgroundColor: colors.coral },
  restrictedCopy: { flex: 1, minWidth: 0 },
  restrictedTitle: { color: colors.ink, fontSize: 14, lineHeight: 20, fontWeight: '700', letterSpacing: 0 },
  restrictedMeta: { color: colors.coral, fontSize: 11, marginTop: 4, letterSpacing: 0 },
  appealDecision: { marginTop: 12, padding: 11, borderRadius: 6, backgroundColor: colors.background },
  appealDecisionStatus: { color: colors.evergreen, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  appealDecisionText: { color: colors.muted, fontSize: 12, lineHeight: 18, marginTop: 4, letterSpacing: 0 },
  appealInput: { minHeight: 92, marginTop: 13, padding: 11, color: colors.ink, fontSize: 13, lineHeight: 20, borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.background, letterSpacing: 0 },
  appealSubmitRow: { minHeight: 36, marginTop: 8, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  appealCharacterCount: { color: colors.faint, fontSize: 10, letterSpacing: 0 },
  appealSubmit: { minHeight: 34, paddingHorizontal: 13, justifyContent: 'center', borderRadius: 5, backgroundColor: colors.evergreen },
  appealSubmitDisabled: { opacity: 0.45 },
  appealSubmitText: { color: colors.surface, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  appealSubmitError: { color: colors.coral, fontSize: 11, lineHeight: 17, marginTop: 6, letterSpacing: 0 },
  appealHistorySection: { color: colors.ink, fontSize: 13, fontWeight: '700', marginTop: 22, marginBottom: 5, letterSpacing: 0 },
  appealHistory: { paddingVertical: 12, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  appealHistoryHeader: { flexDirection: 'row', alignItems: 'center', gap: 10 },
  appealHistoryTitle: { flex: 1, minWidth: 0, color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  appealHistoryStatus: { color: colors.evergreen, fontSize: 10, fontWeight: '700', letterSpacing: 0 },
  appealHistoryDetails: { color: colors.muted, fontSize: 12, lineHeight: 18, marginTop: 5, letterSpacing: 0 },
  appealHistoryResolution: { color: colors.ink, fontSize: 12, lineHeight: 18, marginTop: 5, letterSpacing: 0 },
  appealHistoryDate: { color: colors.faint, fontSize: 10, marginTop: 7, letterSpacing: 0 },
  prompt: { padding: 13, marginBottom: 8, borderLeftWidth: 3, borderLeftColor: colors.gold, backgroundColor: colors.goldSoft },
  promptText: { color: colors.ink, fontSize: 13, lineHeight: 20, letterSpacing: 0 },
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
  settingStatus: { marginTop: -5, marginBottom: 6, color: colors.muted, fontSize: 11, lineHeight: 17, letterSpacing: 0 },
  danger: { padding: 14, marginTop: 24, flexDirection: 'row', gap: 10, borderRadius: 7, backgroundColor: colors.coralSoft },
  dangerText: { flex: 1, color: colors.coral, fontSize: 12, lineHeight: 19, letterSpacing: 0 },
  export: { minHeight: 56, marginTop: 17, paddingHorizontal: 14, flexDirection: 'row', alignItems: 'center', gap: 10, borderRadius: 7, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  exportText: { color: colors.evergreen, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  exportPreview: { marginTop: 12, padding: 12, color: colors.muted, borderRadius: 7, backgroundColor: colors.surface, fontFamily: 'monospace', fontSize: 10, lineHeight: 15 },
  pressed: { opacity: 0.62 },
});
