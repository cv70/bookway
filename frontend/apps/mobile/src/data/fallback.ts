import { Feed, Journey, Today } from '../types';

export const fallbackToday: Today = {
  completed: 1,
  total: 3,
  focus_minutes: 8,
  actions: [
    {
      id: 'action-read-city',
      journey_id: 'journey-reading',
      title: '阅读第三章',
      detail: '标记一个关于城市空间的观点',
      estimated_minutes: 30,
      scheduled_label: '上午',
      state: 'pending',
    },
    {
      id: 'action-easy-run',
      journey_id: 'journey-running',
      title: '轻松跑 25 分钟',
      detail: '保持可以自然说话的配速',
      estimated_minutes: 25,
      scheduled_label: '傍晚',
      state: 'pending',
    },
    {
      id: 'action-stretch',
      journey_id: 'journey-running',
      title: '跑后拉伸',
      detail: '完成小腿与髋部拉伸',
      estimated_minutes: 8,
      scheduled_label: '傍晚',
      state: 'completed',
    },
  ],
};

export const fallbackJourneys: Journey[] = [
  {
    id: 'journey-reading',
    title: '读懂现代城市',
    intent: '用阅读建立观察一座城市的方法',
    domain: 'learning',
    status: 'active',
    progress: 36,
    duration_label: '6 周',
    next_action: '阅读《看不见的城市》第三章',
    participant_count: 1284,
  },
  {
    id: 'journey-running',
    title: '重新跑起来',
    intent: '以不受伤的方式恢复规律运动',
    domain: 'movement',
    status: 'active',
    progress: 58,
    duration_label: '4 周',
    next_action: '轻松跑 25 分钟',
    participant_count: 3276,
  },
];

export const fallbackFeed: Feed = {
  request_id: 'local-preview',
  meta: { sourced: 5, filtered: 0, selected: 5 },
  items: [
    {
      score: 4.82,
      source: 'recommend-main:quality',
      reasons: ['符合你的旅行兴趣', '4862 人正在同行'],
      post: {
        id: 'post-city-walk',
        author_name: '木川',
        author_avatar_url:
          'https://images.unsplash.com/photo-1500648767791-00dcc994a43e?w=160&h=160&fit=crop',
        title: '我用 7 次散步重新认识了杭州',
        summary: '不赶景点，只沿着水系和旧城慢慢走。每次回来，我都画一张自己的城市地图。',
        domain: 'travel',
        cover_url:
          'https://images.unsplash.com/photo-1537531383496-f4749b8032cf?w=1200&h=900&fit=crop',
        route_title: '7 次城市观察散步',
        route_duration: '3 周',
        join_count: 4862,
        like_count: 9128,
        freshness: 0.94,
        tags: ['城市漫游', '观察'],
      },
    },
    {
      score: 4.78,
      source: 'recommend-main:quality',
      reasons: ['符合你的学习兴趣', '7130 人正在同行'],
      post: {
        id: 'post-reading',
        author_name: '一册',
        author_avatar_url:
          'https://images.unsplash.com/photo-1494790108377-be9c29b29330?w=160&h=160&fit=crop',
        title: '读完 12 本书后，我留下了这套主题阅读法',
        summary: '从问题出发选择三本结构不同的书，每周只整理一个能用于生活的结论。',
        domain: 'learning',
        cover_url:
          'https://images.unsplash.com/photo-1495446815901-a7297e633e8d?w=1200&h=900&fit=crop',
        route_title: '四周主题阅读实验',
        route_duration: '4 周',
        join_count: 7130,
        like_count: 15420,
        freshness: 0.88,
        tags: ['阅读', '知识管理'],
      },
    },
    {
      score: 4.75,
      source: 'recommend-main:quality',
      reasons: ['符合你的运动兴趣', '9854 人正在同行'],
      post: {
        id: 'post-running',
        author_name: '长风',
        author_avatar_url:
          'https://images.unsplash.com/photo-1534528741775-53994a69daeb?w=160&h=160&fit=crop',
        title: '从跑不动两公里，到享受清晨的五公里',
        summary: '真正有用的不是逼自己更快，而是给身体足够的恢复时间，并记录每次感受。',
        domain: 'movement',
        cover_url:
          'https://images.unsplash.com/photo-1552674605-db6ffd4facb5?w=1200&h=900&fit=crop',
        route_title: '零压力晨跑计划',
        route_duration: '6 周',
        join_count: 9854,
        like_count: 22180,
        freshness: 0.91,
        tags: ['跑步', '晨间'],
      },
    },
    {
      score: 2.43,
      source: 'recommend-main:quality',
      reasons: ['2176 人正在同行'],
      post: {
        id: 'post-pottery',
        author_name: '未名',
        author_avatar_url:
          'https://images.unsplash.com/photo-1531123897727-8f129e1688ce?w=160&h=160&fit=crop',
        title: '周末做陶，让时间重新慢下来',
        summary: '手上的泥总有自己的脾气。两个周末之后，我不再急着控制最后的样子。',
        domain: 'leisure',
        cover_url:
          'https://images.unsplash.com/photo-1610701596007-11502861dcfa?w=1200&h=900&fit=crop',
        route_title: '陶艺初体验',
        route_duration: '2 周',
        join_count: 2176,
        like_count: 6890,
        freshness: 0.96,
        tags: ['手作', '放松'],
      },
    },
  ],
};
